//! The process environment, captured once by `main` and passed down.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::error::Error;

/// Where the current directory comes from.
#[derive(Debug, Clone)]
enum CurrentDir {
    /// A directory fixed when the environment was built.
    Given(PathBuf),
    /// Read from the process each time it is needed.
    Read(fn() -> std::io::Result<PathBuf>),
}

/// Environment variables, current directory and terminal state of one invocation.
///
/// The library reads these values only through `Env`, so tests build one by hand
/// instead of changing the process environment.
#[derive(Debug, Clone)]
pub struct Env {
    vars: BTreeMap<OsString, OsString>,
    current_dir: CurrentDir,
    stdout_is_terminal: bool,
}

impl Env {
    /// Builds an environment from variables, the current directory and whether stdout is a terminal.
    ///
    /// ```
    /// use std::path::{Path, PathBuf};
    /// use taskist::env::Env;
    ///
    /// let env = Env::new(
    ///     [("HOME".into(), "/users/ada".into()), ("USER".into(), "ada".into())],
    ///     PathBuf::from("/work"),
    ///     false,
    /// );
    /// assert_eq!(
    ///     env.database_path()?,
    ///     Path::new("/users/ada/.local/share/taskist/taskist.db")
    /// );
    /// assert_eq!(env.actor(None), "ada");
    /// assert_eq!(env.actor(Some("agent:docs")), "agent:docs");
    /// # Ok::<(), taskist::error::Error>(())
    /// ```
    pub fn new(
        vars: impl IntoIterator<Item = (OsString, OsString)>,
        current_dir: PathBuf,
        stdout_is_terminal: bool,
    ) -> Self {
        Self {
            vars: vars.into_iter().collect(),
            current_dir: CurrentDir::Given(current_dir),
            stdout_is_terminal,
        }
    }

    /// Builds an environment whose current directory is read with `read_current_dir` only
    /// when a command needs it, so commands that do not can run in a directory that
    /// cannot be read.
    pub fn with_current_dir_reader(
        vars: impl IntoIterator<Item = (OsString, OsString)>,
        read_current_dir: fn() -> std::io::Result<PathBuf>,
        stdout_is_terminal: bool,
    ) -> Self {
        Self {
            vars: vars.into_iter().collect(),
            current_dir: CurrentDir::Read(read_current_dir),
            stdout_is_terminal,
        }
    }

    /// The value of a variable, if set.
    pub fn var(&self, name: &str) -> Option<&OsStr> {
        self.vars.get(OsStr::new(name)).map(OsString::as_os_str)
    }

    /// The current directory; an `internal` error when it cannot be read.
    pub fn current_dir(&self) -> Result<PathBuf, Error> {
        match &self.current_dir {
            CurrentDir::Given(dir) => Ok(dir.clone()),
            CurrentDir::Read(read) => read().map_err(|err| {
                Error::Internal(format!("cannot read the current directory: {err}"))
            }),
        }
    }

    /// `path` itself when it is absolute, otherwise `path` below the current directory.
    pub fn absolute(&self, path: &Path) -> Result<PathBuf, Error> {
        if path.is_absolute() {
            Ok(path.to_path_buf())
        } else {
            Ok(self.current_dir()?.join(path))
        }
    }

    /// Whether stdout is a terminal.
    pub const fn stdout_is_terminal(&self) -> bool {
        self.stdout_is_terminal
    }

    /// Whether human output is coloured, by the automatic rules of `anstream`, applied to
    /// the captured variables instead of the process environment. Never when `NO_COLOR` is
    /// set; otherwise always when `CLICOLOR_FORCE` is set; otherwise never when `CLICOLOR`
    /// is `0`; otherwise only when stdout is a terminal and either `TERM` is set to
    /// anything but `dumb`, `CLICOLOR` is set, or `CI` is set. Empty `NO_COLOR` and
    /// `CLICOLOR_FORCE` values count as unset; empty `CLICOLOR`, `TERM` and `CI` values
    /// count as set.
    pub fn colour(&self) -> bool {
        let clicolor = self.var("CLICOLOR").map(|value| value != "0");
        if self.non_empty("NO_COLOR").is_some() {
            false
        } else if self.non_empty("CLICOLOR_FORCE").is_some() {
            true
        } else if clicolor == Some(false) {
            false
        } else {
            let term_supports_colour = self.var("TERM").is_some_and(|term| term != "dumb");
            self.stdout_is_terminal
                && (term_supports_colour || clicolor == Some(true) || self.var("CI").is_some())
        }
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
        assert_eq!(env.current_dir().unwrap(), PathBuf::from("/w"));
        assert!(env.stdout_is_terminal());
    }

    #[test]
    fn the_current_directory_is_read_only_when_asked_for() {
        let env = Env::with_current_dir_reader(
            [],
            || Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
            false,
        );
        let err = env.current_dir().unwrap_err();
        assert!(matches!(err, Error::Internal(_)), "{err:?}");
        assert!(
            err.to_string()
                .starts_with("cannot read the current directory: "),
            "{err}"
        );
        assert_eq!(
            env.absolute(std::path::Path::new("/abs/db")).unwrap(),
            PathBuf::from("/abs/db")
        );
        assert!(env.absolute(std::path::Path::new("rel")).is_err());
        let env = Env::with_current_dir_reader([], || Ok(PathBuf::from("/read")), false);
        assert_eq!(
            env.absolute(std::path::Path::new("rel")).unwrap(),
            PathBuf::from("/read/rel")
        );
    }

    #[test]
    fn colour_follows_the_automatic_rules_of_anstream() {
        let with = |vars: &[(&str, &str)], terminal: bool| {
            Env::new(
                vars.iter()
                    .map(|(name, value)| ((*name).into(), (*value).into())),
                PathBuf::from("/"),
                terminal,
            )
            .colour()
        };
        let term = ("TERM", "xterm-256color");
        // Calibration: a colour terminal is coloured and a pipe is not.
        assert!(with(&[term], true));
        assert!(!with(&[term], false));
        // The terminal must declare colour support through TERM, CLICOLOR or CI.
        assert!(!with(&[], true));
        assert!(!with(&[("TERM", "dumb")], true));
        assert!(with(&[("TERM", "")], true));
        assert!(with(&[("CLICOLOR", "1")], true));
        assert!(with(&[("TERM", "dumb"), ("CLICOLOR", "1")], true));
        assert!(with(&[("CI", "")], true));
        assert!(!with(&[("CLICOLOR", "1")], false));
        // CLICOLOR=0 and NO_COLOR turn colour off on a terminal.
        assert!(!with(&[term, ("CLICOLOR", "0")], true));
        assert!(!with(&[term, ("NO_COLOR", "1")], true));
        assert!(with(&[term, ("NO_COLOR", "")], true));
        // CLICOLOR_FORCE colours a pipe and a dumb terminal, unless NO_COLOR is set.
        assert!(with(&[("CLICOLOR_FORCE", "1")], false));
        assert!(with(&[("CLICOLOR_FORCE", "1"), ("CLICOLOR", "0")], false));
        assert!(with(&[("CLICOLOR_FORCE", "1"), ("TERM", "dumb")], true));
        assert!(!with(&[("CLICOLOR_FORCE", "")], false));
        assert!(!with(&[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")], true));
        assert!(!with(&[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")], false));
        assert!(with(&[("NO_COLOR", ""), ("CLICOLOR_FORCE", "1")], false));
    }

    #[test]
    fn empty_colour_variables_follow_anstream() {
        let with = |vars: &[(&str, &str)], terminal: bool| {
            Env::new(
                vars.iter()
                    .map(|(name, value)| ((*name).into(), (*value).into())),
                PathBuf::from("/"),
                terminal,
            )
            .colour()
        };
        // Calibration: an unset environment is not coloured, on a terminal or a pipe.
        assert!(!with(&[], true));
        assert!(!with(&[], false));
        // Empty NO_COLOR and CLICOLOR_FORCE count as unset.
        assert!(with(&[("TERM", "xterm"), ("NO_COLOR", "")], true));
        assert!(!with(&[("CLICOLOR_FORCE", "")], false));
        assert!(!with(&[("CLICOLOR_FORCE", "")], true));
        // Empty TERM, CI and CLICOLOR count as set and allow colour on a terminal.
        assert!(with(&[("TERM", "")], true));
        assert!(with(&[("CI", "")], true));
        assert!(with(&[("CLICOLOR", "")], true));
        assert!(with(&[("TERM", "dumb"), ("CLICOLOR", "")], true));
        assert!(!with(&[("TERM", ""), ("CI", ""), ("CLICOLOR", "")], false));
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
