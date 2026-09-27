//! Scope resolution: which project a command targets.
//!
//! Order: the `-p/--project` flag, `TASKIST_PROJECT`, the current directory, no project.
//! `--all-projects` skips the variable and the directory and conflicts with the flag.

use std::path::Path;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::env::Env;
use crate::error::Error;
use crate::model::{Project, validate_name};
use crate::store::Tx;

/// Where the resolved project came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The `-p/--project` flag.
    Flag,
    /// The `TASKIST_PROJECT` variable.
    Env,
    /// The current directory.
    Cwd,
    /// No project.
    None,
}

/// What the command line asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Request<'a> {
    /// The `-p/--project` value.
    pub project: Option<&'a str>,
    /// Whether `--all-projects` was given.
    pub all_projects: bool,
}

/// The resolved scope: a project, or none, and where it came from.
///
/// Serialized as `{project, source}` with the project name or null.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// The project, if one was resolved.
    pub project: Option<Project>,
    /// Where it came from; [`Source::None`] exactly when there is no project.
    pub source: Source,
}

impl Serialize for Scope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("Scope", 2)?;
        state.serialize_field("project", &self.project.as_ref().map(|p| p.name.as_str()))?;
        state.serialize_field("source", &self.source)?;
        state.end()
    }
}

impl Scope {
    /// The project, or a `usage` error naming `-p/--project` for commands that need one.
    pub fn require(self) -> Result<Project, Error> {
        self.project.ok_or_else(|| {
            Error::Usage(
                "no project: pass -p/--project, set TASKIST_PROJECT, \
                 or run inside a registered project directory"
                    .into(),
            )
        })
    }
}

/// The project a request asks for, with any name in it already validated.
///
/// Built by [`target`] before the store is opened, so an invalid name is a `usage` error
/// whatever state the database is in; [`resolve`] takes only this type, so no name reaches
/// a lookup unvalidated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pick: Pick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pick {
    /// `--all-projects`: no project.
    All,
    /// A project name from the flag or the variable.
    Named(String, Source),
    /// The project linked to the current directory, if any.
    Directory,
}

/// Decides where the project of a request comes from and validates its name, without
/// reading the database.
pub fn target(env: &Env, request: Request<'_>) -> Result<Target, Error> {
    let pick = if request.all_projects {
        if request.project.is_some() {
            return Err(Error::Usage(
                "--all-projects cannot be used with -p/--project".into(),
            ));
        }
        Pick::All
    } else if let Some(name) = request.project {
        Pick::Named(name.to_owned(), Source::Flag)
    } else if let Some(name) = env.var("TASKIST_PROJECT").filter(|value| !value.is_empty()) {
        Pick::Named(name.to_string_lossy().into_owned(), Source::Env)
    } else {
        Pick::Directory
    };
    if let Pick::Named(name, _) = &pick {
        validate_name("project name", name)?;
    }
    Ok(Target { pick })
}

/// Resolves the scope of one command.
pub fn resolve(tx: &Tx<'_>, env: &Env, target: &Target) -> Result<Scope, Error> {
    match &target.pick {
        Pick::All => Ok(none()),
        Pick::Named(name, source) => named(tx, name, *source),
        Pick::Directory => {
            let current_dir = env.current_dir()?;
            let cwd = std::fs::canonicalize(&current_dir).map_err(|err| {
                Error::Internal(format!(
                    "cannot resolve the current directory {}: {err}",
                    current_dir.display()
                ))
            })?;
            Ok(
                match_directory(tx.projects(false)?, &cwd).map_or_else(none, |project| Scope {
                    project: Some(project),
                    source: Source::Cwd,
                }),
            )
        }
    }
}

const fn none() -> Scope {
    Scope {
        project: None,
        source: Source::None,
    }
}

fn named(tx: &Tx<'_>, name: &str, source: Source) -> Result<Scope, Error> {
    let project = tx
        .project_by_name(name)?
        .ok_or_else(|| Error::NotFound(format!("no project named {name:?}")))?;
    Ok(Scope {
        project: Some(project),
        source,
    })
}

/// The non-archived project whose path is the longest component-wise prefix of `dir`.
///
/// Both `dir` and the stored paths are canonical, so a string comparison is not needed
/// and would be wrong: `/code/web` is not a prefix of `/code/webapp`.
fn match_directory(projects: Vec<Project>, dir: &Path) -> Option<Project> {
    projects
        .into_iter()
        .filter(|project| !project.archived)
        .filter_map(|project| {
            let depth = project
                .path
                .as_deref()
                .map(Path::new)
                .filter(|path| dir.starts_with(path))?
                .components()
                .count();
            Some((depth, project))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, project)| project)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::{Scope, Source, match_directory};
    use crate::error::Error;
    use crate::model::Project;

    fn project(name: &str, path: Option<&str>, archived: bool) -> Project {
        Project {
            id: 1,
            name: name.into(),
            path: path.map(str::to_owned),
            description: String::new(),
            archived,
            created_at: String::new(),
        }
    }

    fn matched(projects: Vec<Project>, dir: &str) -> Option<String> {
        match_directory(projects, Path::new(dir)).map(|project| project.name)
    }

    #[test]
    fn the_longest_component_prefix_wins_in_any_order() {
        let projects = || {
            vec![
                project("inner", Some("/code/web"), false),
                project("outer", Some("/code"), false),
                project("none", None, false),
            ]
        };
        assert_eq!(
            matched(projects(), "/code/web/src").as_deref(),
            Some("inner")
        );
        assert_eq!(matched(projects(), "/code/web").as_deref(), Some("inner"));
        assert_eq!(matched(projects(), "/code/api").as_deref(), Some("outer"));
        let mut reversed = projects();
        reversed.reverse();
        assert_eq!(matched(reversed, "/code/web/src").as_deref(), Some("inner"));
        assert_eq!(matched(projects(), "/elsewhere"), None);
    }

    #[test]
    fn a_shared_name_prefix_is_not_a_path_prefix() {
        let projects = vec![project("web", Some("/code/web"), false)];
        assert_eq!(matched(projects, "/code/webapp"), None);
    }

    #[test]
    fn archived_projects_never_match() {
        let projects = vec![
            project("outer", Some("/code"), false),
            project("web", Some("/code/web"), true),
        ];
        assert_eq!(matched(projects, "/code/web").as_deref(), Some("outer"));
    }

    #[test]
    fn scope_serializes_the_project_name_and_the_source() {
        let scope = Scope {
            project: Some(project("web", None, false)),
            source: Source::Env,
        };
        assert_eq!(
            serde_json::to_value(&scope).unwrap(),
            json!({"project": "web", "source": "env"})
        );
        let none = super::none();
        assert_eq!(
            serde_json::to_value(&none).unwrap(),
            json!({"project": null, "source": "none"})
        );
        let err = none.require().unwrap_err();
        assert!(matches!(err, Error::Usage(_)), "{err:?}");
        assert!(err.to_string().contains("-p/--project"), "{err}");
    }
}
