//! Command implementations: each opens the store, runs one transaction and returns data
//! for the output module.

pub mod feature;
pub mod project;

use std::collections::HashMap;

use serde::Serialize;

use crate::env::Env;
use crate::error::Error;
use crate::model::{Project, Status, Task};
use crate::scope::{self, Request, Scope};
use crate::store::{Store, Tx};

/// Opens the database at the location the environment resolves, relative paths from the
/// captured current directory.
fn open_store(env: &Env) -> Result<Store, Error> {
    Store::open(&env.current_dir().join(env.database_path()?))
}

/// A project as the output shows it: `{name, path, description, archived, created_at, open}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectView {
    /// Unique name.
    pub name: String,
    /// Linked directory, canonical, if any.
    pub path: Option<String>,
    /// Free text.
    pub description: String,
    /// Whether the project is archived.
    pub archived: bool,
    /// Creation time, UTC.
    pub created_at: String,
    /// Number of open tasks.
    pub open: i64,
}

/// A feature as the output shows it: `{project, name, open, total}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureView {
    /// Owning project name.
    pub project: String,
    /// Feature name.
    pub name: String,
    /// Number of open tasks.
    pub open: i64,
    /// Number of tasks in any status.
    pub total: i64,
}

/// A task as the output shows it, with project and feature names instead of ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskView {
    /// Global id.
    pub id: i64,
    /// Owning project name.
    pub project: String,
    /// Feature name, if any.
    pub feature: Option<String>,
    /// Single-line title.
    pub title: String,
    /// Free text.
    pub body: String,
    /// Current status.
    pub status: Status,
    /// 0 (most urgent) to 3.
    pub priority: u8,
    /// Tags, sorted.
    pub tags: Vec<String>,
    /// Creation time, UTC.
    pub created_at: String,
    /// Time of the last change, UTC.
    pub updated_at: String,
    /// Time the task became `done` or `dropped`.
    pub closed_at: Option<String>,
    /// Actor that created the task.
    pub created_by: String,
}

/// `tk ls` data: `{scope, tasks}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskList {
    /// The resolved scope.
    pub scope: Scope,
    /// Open tasks, by priority, then age.
    pub tasks: Vec<TaskView>,
}

/// The task counts of one project, per feature and status.
struct Tally {
    rows: Vec<(Option<i64>, Status, i64)>,
}

impl Tally {
    fn of(tx: &Tx<'_>, project_id: i64) -> Result<Self, Error> {
        Ok(Self {
            rows: tx.task_counts(project_id)?,
        })
    }

    fn sum(&self, keep: impl Fn(Option<i64>, Status) -> bool) -> i64 {
        self.rows
            .iter()
            .filter(|(feature, status, _)| keep(*feature, *status))
            .map(|(_, _, count)| count)
            .sum()
    }

    fn open(&self) -> i64 {
        self.sum(|_, status| status.is_open())
    }

    fn total(&self) -> i64 {
        self.sum(|_, _| true)
    }

    fn status(&self, wanted: Status) -> i64 {
        self.sum(|_, status| status == wanted)
    }

    /// Open and total counts of one feature.
    fn feature(&self, id: i64) -> (i64, i64) {
        (
            self.sum(|feature, status| feature == Some(id) && status.is_open()),
            self.sum(|feature, _| feature == Some(id)),
        )
    }
}

fn project_view(tx: &Tx<'_>, project: Project) -> Result<ProjectView, Error> {
    let open = Tally::of(tx, project.id)?.open();
    Ok(ProjectView {
        name: project.name,
        path: project.path,
        description: project.description,
        archived: project.archived,
        created_at: project.created_at,
        open,
    })
}

fn project_named(tx: &Tx<'_>, name: &str) -> Result<Project, Error> {
    tx.project_by_name(name)?
        .ok_or_else(|| Error::NotFound(format!("no project named {name:?}")))
}

/// The projects a listing covers: the scoped one, or every non-archived project.
fn listed_projects(tx: &Tx<'_>, scope: &Scope) -> Result<Vec<Project>, Error> {
    scope
        .project
        .as_ref()
        .map_or_else(|| tx.projects(false), |project| Ok(vec![project.clone()]))
}

/// `tk ls`: the open tasks of the resolved scope, by priority, then age.
pub fn list(env: &Env, request: Request<'_>) -> Result<TaskList, Error> {
    open_store(env)?.read(|tx| {
        let scope = scope::resolve(tx, env, request)?;
        let mut tasks = Vec::new();
        for project in listed_projects(tx, &scope)? {
            let features: HashMap<i64, String> = tx
                .features(project.id)?
                .into_iter()
                .map(|feature| (feature.id, feature.name))
                .collect();
            for task in tx.tasks(project.id)? {
                if task.status.is_open() {
                    tasks.push(task_view(task, &project.name, &features));
                }
            }
        }
        tasks.sort_by(|a, b| {
            (a.priority, &a.created_at, a.id).cmp(&(b.priority, &b.created_at, b.id))
        });
        Ok(TaskList { scope, tasks })
    })
}

fn task_view(task: Task, project: &str, features: &HashMap<i64, String>) -> TaskView {
    TaskView {
        id: task.id,
        project: project.to_owned(),
        feature: task.feature_id.and_then(|id| features.get(&id).cloned()),
        title: task.title,
        body: task.body,
        status: task.status,
        priority: task.priority,
        tags: task.tags,
        created_at: task.created_at,
        updated_at: task.updated_at,
        closed_at: task.closed_at,
        created_by: task.created_by,
    }
}
