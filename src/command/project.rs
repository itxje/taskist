//! The `tk project` commands.

use std::path::Path;

use serde::Serialize;

use super::{FeatureView, ProjectView, Tally, open_store, project_named, project_view};
use crate::env::Env;
use crate::error::Error;
use crate::model::{Status, validate_name};

/// `project add`, `project edit` data: `{project}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectData {
    /// The project after the change.
    pub project: ProjectView,
}

/// `project ls` data: `{projects}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectList {
    /// Projects ordered by name.
    pub projects: Vec<ProjectView>,
}

/// Task counts per status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StatusCounts {
    /// `todo` tasks.
    pub todo: i64,
    /// `doing` tasks.
    pub doing: i64,
    /// `blocked` tasks.
    pub blocked: i64,
    /// `done` tasks.
    pub done: i64,
    /// `dropped` tasks.
    pub dropped: i64,
}

/// `project show` data: `{project, features, counts}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectShow {
    /// The project.
    pub project: ProjectView,
    /// Its features ordered by name.
    pub features: Vec<FeatureView>,
    /// Its task counts per status.
    pub counts: StatusCounts,
}

/// `project archive` data: `{project, changed}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectArchive {
    /// The project after the command.
    pub project: ProjectView,
    /// Whether the archived state changed; false when it already was the requested one.
    pub changed: bool,
}

/// `project rm` data: `{removed, tasks}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectRemoval {
    /// Name of the deleted project.
    pub removed: String,
    /// Number of tasks deleted with it.
    pub tasks: i64,
}

/// The changes `project edit` asks for.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectEdit<'a> {
    /// New name.
    pub name: Option<&'a str>,
    /// New linked directory.
    pub path: Option<&'a Path>,
    /// Remove the linked directory.
    pub no_path: bool,
    /// New description.
    pub description: Option<&'a str>,
}

/// Canonicalizes a directory given on the command line, relative to the current directory.
///
/// It must exist, be a directory and have a UTF-8 path, since paths are stored as text.
fn project_dir(env: &Env, dir: &Path) -> Result<String, Error> {
    let canonical = std::fs::canonicalize(env.absolute(dir)?).map_err(|err| {
        Error::Usage(format!(
            "cannot use {} as a project directory: {err}",
            dir.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(Error::Usage(format!(
            "{} is not a directory",
            canonical.display()
        )));
    }
    canonical.into_os_string().into_string().map_err(|path| {
        Error::Usage(format!(
            "the project directory {} is not valid UTF-8",
            Path::new(&path).display()
        ))
    })
}

fn refuse_used_name(tx: &crate::store::Tx<'_>, name: &str) -> Result<(), Error> {
    if tx.project_by_name(name)?.is_some() {
        return Err(Error::Conflict(format!(
            "a project named {name:?} already exists"
        )));
    }
    Ok(())
}

/// Refuses a path linked to a project other than `own`.
fn refuse_used_path(tx: &crate::store::Tx<'_>, path: &str, own: Option<i64>) -> Result<(), Error> {
    match tx.project_by_path(path)? {
        Some(other) if Some(other.id) != own => Err(Error::Conflict(format!(
            "project {:?} already uses the directory {path}",
            other.name
        ))),
        _ => Ok(()),
    }
}

/// `tk project add`.
pub fn add(
    env: &Env,
    name: &str,
    path: Option<&Path>,
    description: Option<&str>,
) -> Result<ProjectData, Error> {
    validate_name("project name", name)?;
    let path = path.map(|dir| project_dir(env, dir)).transpose()?;
    open_store(env)?.write(|tx| {
        refuse_used_name(tx, name)?;
        if let Some(path) = &path {
            refuse_used_path(tx, path, None)?;
        }
        let project = tx.insert_project(name, path.as_deref(), description.unwrap_or(""))?;
        Ok(ProjectData {
            project: project_view(tx, project)?,
        })
    })
}

/// `tk project ls`: projects ordered by name; archived ones only with `all`.
pub fn list(env: &Env, all: bool) -> Result<ProjectList, Error> {
    open_store(env)?.read(|tx| {
        let projects = tx
            .projects(all)?
            .into_iter()
            .map(|project| project_view(tx, project))
            .collect::<Result<_, _>>()?;
        Ok(ProjectList { projects })
    })
}

/// `tk project show`.
pub fn show(env: &Env, name: &str) -> Result<ProjectShow, Error> {
    validate_name("project name", name)?;
    open_store(env)?.read(|tx| {
        let project = project_named(tx, name)?;
        let tally = Tally::of(tx, project.id)?;
        let features = tx
            .features(project.id)?
            .into_iter()
            .map(|feature| {
                let (open, total) = tally.feature(feature.id);
                FeatureView {
                    project: project.name.clone(),
                    name: feature.name,
                    open,
                    total,
                }
            })
            .collect();
        let counts = StatusCounts {
            todo: tally.status(Status::Todo),
            doing: tally.status(Status::Doing),
            blocked: tally.status(Status::Blocked),
            done: tally.status(Status::Done),
            dropped: tally.status(Status::Dropped),
        };
        Ok(ProjectShow {
            project: project_view(tx, project)?,
            features,
            counts,
        })
    })
}

/// `tk project edit`: at least one change is required.
pub fn edit(env: &Env, name: &str, edit: ProjectEdit<'_>) -> Result<ProjectData, Error> {
    if edit.name.is_none() && edit.path.is_none() && !edit.no_path && edit.description.is_none() {
        return Err(Error::Usage(
            "nothing to change: pass --name, --path, --no-path or --desc".into(),
        ));
    }
    validate_name("project name", name)?;
    if let Some(new_name) = edit.name {
        validate_name("project name", new_name)?;
    }
    let path = edit.path.map(|dir| project_dir(env, dir)).transpose()?;
    open_store(env)?.write(|tx| {
        let project = project_named(tx, name)?;
        let new_name = edit.name.unwrap_or(&project.name);
        if new_name != project.name {
            refuse_used_name(tx, new_name)?;
        }
        if let Some(path) = &path {
            refuse_used_path(tx, path, Some(project.id))?;
        }
        let new_path = if edit.no_path {
            None
        } else {
            path.or_else(|| project.path.clone())
        };
        let updated = tx.update_project(
            project.id,
            new_name,
            new_path.as_deref(),
            edit.description.unwrap_or(&project.description),
        )?;
        Ok(ProjectData {
            project: project_view(tx, updated)?,
        })
    })
}

/// `tk project archive`: archives, or unarchives with `undo`; idempotent.
pub fn archive(env: &Env, name: &str, undo: bool) -> Result<ProjectArchive, Error> {
    validate_name("project name", name)?;
    open_store(env)?.write(|tx| {
        let mut project = project_named(tx, name)?;
        let archived = !undo;
        let changed = project.archived != archived;
        if changed {
            tx.set_project_archived(project.id, archived)?;
            project.archived = archived;
        }
        Ok(ProjectArchive {
            project: project_view(tx, project)?,
            changed,
        })
    })
}

/// `tk project rm`: a project with tasks needs `force`, which deletes them with it.
pub fn remove(env: &Env, name: &str, force: bool) -> Result<ProjectRemoval, Error> {
    validate_name("project name", name)?;
    open_store(env)?.write(|tx| {
        let project = project_named(tx, name)?;
        let tasks = Tally::of(tx, project.id)?.total();
        if tasks > 0 && !force {
            return Err(Error::Conflict(format!(
                "project {} has {tasks} tasks: pass --force to delete them with the project",
                project.name
            )));
        }
        tx.delete_project(project.id)?;
        Ok(ProjectRemoval {
            removed: project.name,
            tasks,
        })
    })
}
