//! Shared helper for integration tests that run the `tk` binary.
//!
//! Every command starts from an empty environment inside a fresh temporary
//! directory, so no test can reach the user's database even when the
//! `TASKIST_DB` handling is broken.
#![cfg(test)]
#![allow(
    dead_code,
    reason = "each integration test crate uses a different part of the helper"
)]

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// A temporary directory holding the database, home, data and working directories of one test.
#[derive(Debug)]
pub struct Sandbox {
    root: TempDir,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Sandbox {
    /// Creates the temporary directory and its `home`, `data` and `work` subdirectories.
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("create temporary directory");
        for dir in ["home", "data", "work"] {
            std::fs::create_dir(root.path().join(dir)).expect("create sandbox directory");
        }
        Self { root }
    }

    /// The temporary directory that contains everything the test may touch.
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// The database file `TASKIST_DB` points to.
    pub fn db(&self) -> PathBuf {
        self.root().join("taskist.db")
    }

    /// The home directory `HOME` points to.
    pub fn home(&self) -> PathBuf {
        self.root().join("home")
    }

    /// The data directory `XDG_DATA_HOME` points to.
    pub fn data_home(&self) -> PathBuf {
        self.root().join("data")
    }

    /// The working directory commands run in.
    pub fn work(&self) -> PathBuf {
        self.root().join("work")
    }

    /// The variables every command gets, `TASKIST_DB` first.
    pub fn vars(&self) -> Vec<(OsString, OsString)> {
        vec![
            ("TASKIST_DB".into(), self.db().into()),
            ("HOME".into(), self.home().into()),
            ("XDG_DATA_HOME".into(), self.data_home().into()),
        ]
    }

    /// A `tk` command with the sandbox environment.
    pub fn tk(&self) -> Command {
        self.program(tk_path())
    }

    /// A `tk` command with the sandbox environment but without `TASKIST_DB`.
    pub fn tk_without_db(&self) -> Command {
        self.program_without_db(tk_path())
    }

    /// Any program, run exactly as `tk` is: the sandbox environment, `work()` as the
    /// working directory, stdin closed. Tests use it to observe what the helper hands a child.
    pub fn program(&self, program: impl AsRef<OsStr>) -> Command {
        self.command(program.as_ref(), self.vars())
    }

    /// Any program, run exactly as `tk_without_db` runs `tk`.
    pub fn program_without_db(&self, program: impl AsRef<OsStr>) -> Command {
        let vars = self
            .vars()
            .into_iter()
            .filter(|(name, _)| name != "TASKIST_DB");
        self.command(program.as_ref(), vars)
    }

    /// The single place that builds a sandboxed command.
    ///
    /// Stdin is an empty input that is closed as soon as the child starts, so a read
    /// sees end-of-file at once; a test that supplies input replaces it with `write_stdin`.
    fn command(
        &self,
        program: &OsStr,
        vars: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> Command {
        let mut cmd = std::process::Command::new(program);
        cmd.env_clear().envs(vars).current_dir(self.work());
        let mut cmd = Command::from_std(cmd);
        cmd.write_stdin("");
        cmd
    }
}

fn tk_path() -> &'static Path {
    assert_cmd::cargo::cargo_bin!("tk")
}
