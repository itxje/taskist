//! Integration tests for opening the database through `tk`.
#![cfg(test)]

pub mod common;

use std::sync::Barrier;
use std::thread;
use std::time::Duration;

use common::Sandbox;
use serde_json::{Value, json};

/// Number of `tk` processes that open one new database at the same time.
const PROCESSES: usize = 8;

/// Upper bound for one `tk` run, well above the database busy timeout.
const RUN_TIMEOUT: Duration = Duration::from_secs(30);

fn json_line(bytes: &[u8]) -> Value {
    let text = std::str::from_utf8(bytes).expect("utf-8 output");
    assert_eq!(text.lines().count(), 1, "one line expected: {text:?}");
    serde_json::from_str(text).expect("valid JSON")
}

#[test]
fn project_ls_on_a_new_database_prints_an_empty_list() {
    let sandbox = Sandbox::new();
    sandbox
        .tk()
        .args(["project", "ls"])
        .assert()
        .code(0)
        .stdout("no projects\n")
        .stderr("");
    let output = sandbox
        .tk()
        .args(["project", "ls", "--json"])
        .output()
        .expect("run tk");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(json_line(&output.stdout), json!({"ok": true, "data": []}));
}

#[test]
fn project_ls_refuses_a_newer_schema() {
    let sandbox = Sandbox::new();
    {
        let conn = rusqlite::Connection::open(sandbox.db()).expect("create database");
        conn.pragma_update(None, "user_version", 2)
            .expect("set user_version");
    }
    let before = std::fs::read(sandbox.db()).expect("read database");
    let output = sandbox
        .tk()
        .args(["--json", "project", "ls"])
        .output()
        .expect("run tk");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        "unsupported_schema"
    );
    assert_eq!(std::fs::read(sandbox.db()).expect("read database"), before);
}

#[test]
fn concurrent_first_opens_all_succeed_and_create_the_schema_once() {
    let sandbox = Sandbox::new();
    assert!(!sandbox.db().exists());
    let barrier = Barrier::new(PROCESSES);
    let outputs: Vec<std::process::Output> = thread::scope(|scope| {
        let runs: Vec<_> = (0..PROCESSES)
            .map(|_| {
                let mut cmd = sandbox.tk();
                cmd.args(["--json", "project", "ls"]).timeout(RUN_TIMEOUT);
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    cmd.output().expect("run tk")
                })
            })
            .collect();
        runs.into_iter()
            .map(|run| run.join().expect("thread finished"))
            .collect()
    });
    assert_eq!(outputs.len(), PROCESSES);
    for output in &outputs {
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(json_line(&output.stdout), json!({"ok": true, "data": []}));
    }

    let conn = rusqlite::Connection::open(sandbox.db()).expect("open database");
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version");
    assert_eq!(version, 1);
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("prepare");
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(
        tables,
        [
            "feature",
            "note",
            "project",
            "sqlite_sequence",
            "task",
            "task_tag"
        ]
    );
}
