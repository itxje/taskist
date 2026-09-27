//! The process environment, captured once by `main` and passed down.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::error::Error;

/// Environment variables, current directory and terminal state of one invocation.
///
/// The library reads these values only through `Env`, so tests build one by hand
/// instead of changing the process environment.
#[derive(Debug, Clone)]
pub struct Env {
    vars: BTreeMap<OsString, OsString>,
    current_dir: PathBuf,
    stdout_is_terminal: bool,
}

impl Env {
    /// Builds an environment from variables, the current directory and whether stdout is a terminal.
    pub fn new(
        vars: impl IntoIterator<Item = (OsString, OsString)>,
        current_dir: PathBuf,
        stdout_is_terminal: bool,
    ) -> Self {
        Self {
            vars: vars.into_iter().collect(),
            current_dir,
            stdout_is_terminal,
        }
    }

    /// The value of a variable, if set.
    pub fn var(&self, name: &str) -> Option<&OsStr> {
        self.vars.get(OsStr::new(name)).map(OsString::as_os_str)
    }

    /// The current directory at startup.
    pub fn current_dir(&self) -> &Path {
        &self.current_dir
    }

    /// Whether stdout is a terminal.
    pub const fn stdout_is_terminal(&self) -> bool {
        self.stdout_is_terminal
    }

    /// The value of a variable, if set to a non-empty value.
    fn non_empty(&self, name: &str) -> Option<&Path> {
        self.var(name)
            .filter(|value| !value.is_empty())
            .map(Path::new)
    }

    /// The database file location, first match wins: `TASKIST_DB` as given,
    /// `$XDG_DATA_HOME/taskist/taskist.db` when `XDG_DATA_HOME` is absolute,
    /// `$HOME/.local/share/taskist/taskist.db` when `HOME` is absolute.
    ///
    /// Empty values count as unset. A relative `HOME` is not used, because the
    /// location would then depend on the current directory.
    pub fn database_path(&self) -> Result<PathBuf, Error> {
        if let Some(path) = self.non_empty("TASKIST_DB") {
            return Ok(path.to_path_buf());
        }
        let data_home = self
            .non_empty("XDG_DATA_HOME")
            .filter(|dir| dir.is_absolute())
            .map(Path::to_path_buf)
            .or_else(|| {
                self.non_empty("HOME")
                    .filter(|dir| dir.is_absolute())
                    .map(|home| home.join(".local").join("share"))
            })
            .ok_or_else(|| {
                Error::Usage(
                    "cannot locate the database: set TASKIST_DB, or an absolute XDG_DATA_HOME or HOME"
                        .into(),
                )
            })?;
        Ok(data_home.join("taskist").join("taskist.db"))
    }

    /// The actor stored as `created_by` and `author`, first match wins: the `--by` value,
    /// `TASKIST_ACTOR`, `USER`, the literal `unknown`. Empty values count as unset.
    pub fn actor(&self, by: Option<&str>) -> String {
        by.filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                ["TASKIST_ACTOR", "USER"]
                    .into_iter()
                    .find_map(|name| self.non_empty(name))
                    .map(|value| value.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "unknown".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::Env;
    use crate::error::Error;

    fn env(vars: &[(&str, &str)]) -> Env {
        Env::new(
            vars.iter()
                .map(|(name, value)| ((*name).into(), (*value).into())),
            PathBuf::from("/work"),
            false,
        )
    }

    #[test]
    fn accessors_return_the_captured_values() {
        let env = Env::new([("A".into(), "1".into())], PathBuf::from("/w"), true);
        assert_eq!(env.var("A"), Some("1".as_ref()));
        assert_eq!(env.var("B"), None);
        assert_eq!(env.current_dir(), PathBuf::from("/w"));
        assert!(env.stdout_is_terminal());
    }

    #[test]
    fn taskist_db_wins_and_is_used_as_given() {
        let env = env(&[
            ("TASKIST_DB", "rel/my.db"),
            ("XDG_DATA_HOME", "/xdg"),
            ("HOME", "/users/u"),
        ]);
        assert_eq!(env.database_path().unwrap(), PathBuf::from("rel/my.db"));
    }

    #[test]
    fn absolute_xdg_data_home_comes_next() {
        let env = env(&[("XDG_DATA_HOME", "/xdg"), ("HOME", "/users/u")]);
        assert_eq!(
            env.database_path().unwrap(),
            PathBuf::from("/xdg/taskist/taskist.db")
        );
    }

    #[test]
    fn relative_xdg_data_home_is_ignored() {
        let env = env(&[("XDG_DATA_HOME", "xdg"), ("HOME", "/users/u")]);
        assert_eq!(
            env.database_path().unwrap(),
            PathBuf::from("/users/u/.local/share/taskist/taskist.db")
        );
    }

    #[test]
    fn home_alone_is_enough() {
        let env = env(&[("HOME", "/users/u")]);
        assert_eq!(
            env.database_path().unwrap(),
            PathBuf::from("/users/u/.local/share/taskist/taskist.db")
        );
    }

    #[test]
    fn empty_values_count_as_unset() {
        let env = env(&[
            ("TASKIST_DB", ""),
            ("XDG_DATA_HOME", ""),
            ("HOME", "/users/u"),
        ]);
        assert_eq!(
            env.database_path().unwrap(),
            PathBuf::from("/users/u/.local/share/taskist/taskist.db")
        );
    }

    #[test]
    fn relative_home_is_not_resolvable() {
        let err = env(&[("HOME", "home")]).database_path().unwrap_err();
        assert!(matches!(err, Error::Usage(_)), "{err:?}");
    }

    #[test]
    fn actor_prefers_the_flag_then_taskist_actor_then_user() {
        let all = env(&[("TASKIST_ACTOR", "agent"), ("USER", "alice")]);
        assert_eq!(all.actor(Some("cli")), "cli");
        assert_eq!(all.actor(None), "agent");
        assert_eq!(env(&[("USER", "alice")]).actor(None), "alice");
        assert_eq!(env(&[]).actor(None), "unknown");
    }

    #[test]
    fn empty_actor_values_count_as_unset() {
        let env = env(&[("TASKIST_ACTOR", ""), ("USER", "alice")]);
        assert_eq!(env.actor(Some("")), "alice");
        assert_eq!(
            super::Env::new([], PathBuf::from("/"), false).actor(Some("")),
            "unknown"
        );
    }

    #[test]
    fn nothing_set_is_a_usage_error_naming_taskist_db() {
        let err = env(&[]).database_path().unwrap_err();
        assert!(matches!(err, Error::Usage(_)), "{err:?}");
        assert!(err.to_string().contains("TASKIST_DB"), "{err}");
    }
}
