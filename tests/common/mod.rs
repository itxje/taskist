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
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;
use taskist::model::{NoteKind, Status};
use taskist::store::{NewTask, Store};
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

impl Sandbox {
    /// Creates a directory below the sandbox root, with its missing parents, and returns it.
    pub fn dir(&self, relative: &str) -> PathBuf {
        let dir = self.root().join(relative);
        std::fs::create_dir_all(&dir).expect("create directory");
        dir
    }

    /// Runs `tk --json` with `args` and returns the `data` of its success envelope.
    pub fn ok(&self, args: &[&str]) -> Value {
        ok_data(&self.tk().arg("--json").args(args).output().expect("run tk"))
    }

    /// Opens the sandbox database through the library, to seed rows no command creates yet.
    pub fn store(&self) -> Store {
        Store::open(&self.db()).expect("open database")
    }

    /// Creates a task in a project, in an optional feature created on first use, and
    /// moves it to `status`; returns its id.
    pub fn seed_task(&self, project: &str, feature: Option<&str>, status: Status) -> i64 {
        self.store()
            .write(|tx| {
                let project = tx.project_by_name(project)?.expect("seeded project exists");
                let feature_id = match feature {
                    None => None,
                    Some(name) => Some(match tx.feature_by_name(project.id, name)? {
                        Some(feature) => feature.id,
                        None => tx.insert_feature(project.id, name)?.id,
                    }),
                };
                let task = tx.insert_task(&NewTask {
                    project_id: project.id,
                    feature_id,
                    title: "seeded",
                    body: "",
                    priority: 2,
                    created_by: "seed",
                })?;
                if status != Status::Todo {
                    tx.set_status(task.id, status)?;
                }
                Ok(task.id)
            })
            .expect("seed task")
    }
}

impl Sandbox {
    /// Moves a task to `status` through the library, bypassing the transition table.
    pub fn set_status(&self, id: i64, status: Status) {
        self.store()
            .write(|tx| tx.set_status(id, status))
            .expect("set status");
    }

    /// Appends a note to a task through the library.
    pub fn note(&self, id: i64, kind: NoteKind, text: &str) {
        self.store()
            .write(|tx| tx.insert_note(id, kind, text, "seed").map(drop))
            .expect("add note");
    }

    /// Runs one SQL statement on the sandbox database, to set values no command sets.
    pub fn sql(&self, statement: &str, params: impl rusqlite::Params) {
        rusqlite::Connection::open(self.db())
            .expect("open database")
            .execute(statement, params)
            .expect("run statement");
    }
}

/// Parses output that must be exactly one newline-terminated JSON line.
pub fn json_line(bytes: &[u8]) -> Value {
    let text = std::str::from_utf8(bytes).expect("utf-8 output");
    assert_eq!(text.lines().count(), 1, "one line expected: {text:?}");
    assert!(text.ends_with('\n'), "newline-terminated: {text:?}");
    serde_json::from_str(text).expect("valid JSON")
}

/// Checks a JSON success: exit 0, nothing on stderr, `ok` true; returns `data`.
pub fn ok_data(output: &Output) -> Value {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let envelope = json_line(&output.stdout);
    assert_eq!(envelope["ok"], Value::Bool(true), "{envelope}");
    envelope["data"].clone()
}

/// Checks a JSON failure with `exit` and `code`, nothing on stdout; returns the message.
pub fn err_message(output: &Output, exit: i32, code: &str) -> String {
    assert_eq!(output.status.code(), Some(exit), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let envelope = json_line(&output.stderr);
    assert_eq!(envelope["ok"], Value::Bool(false), "{envelope}");
    assert_eq!(envelope["error"]["code"], code, "{envelope}");
    envelope["error"]["message"]
        .as_str()
        .expect("message")
        .to_owned()
}

/// The canonical form of a directory, as `tk` stores it.
pub fn real(path: &Path) -> String {
    std::fs::canonicalize(path)
        .expect("canonicalize")
        .to_str()
        .expect("utf-8 path")
        .to_owned()
}
