//! An invalid project name is a `usage` error whatever state the database is in, as an
//! invalid feature name or tag already is: the name is checked before the store is opened.
#![cfg(test)]

pub mod common;

use common::{Sandbox, err_message};

/// A sandbox whose database was written by a newer version, so opening it fails.
fn newer_database() -> Sandbox {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);
    sandbox.ok(&["add", "task", "-p", "web", "-f", "auth"]);
    sandbox.sql("PRAGMA user_version = 99", []);
    sandbox
}

fn run(sandbox: &Sandbox, args: &[&str]) -> std::process::Output {
    sandbox
        .tk()
        .arg("--json")
        .args(args)
        .output()
        .expect("run tk")
}

#[test]
fn controls_the_database_is_refused_and_other_names_are_checked_first() {
    let sandbox = newer_database();
    err_message(
        &run(&sandbox, &["ls", "-p", "web"]),
        1,
        "unsupported_schema",
    );
    for args in [
        &["ls", "-f", "BAD_NAME"][..],
        &["ls", "--tag", "BAD_NAME"],
        &["add", "t", "-p", "web", "-f", "BAD_NAME"],
        &["edit", "1", "-f", "BAD_NAME"],
        &["project", "add", "BAD_NAME"],
        &["feature", "mv", "BAD_NAME", "login", "-p", "web"],
    ] {
        err_message(&run(&sandbox, args), 2, "usage");
    }
}

#[test]
fn an_invalid_project_name_is_usage_whatever_the_database_state() {
    let sandbox = newer_database();
    let mut failures = Vec::new();
    for args in [
        &["ls", "-p", "BAD_NAME"][..],
        &["next", "-p", "BAD_NAME"],
        &["find", "task", "-p", "BAD_NAME"],
        &["feature", "ls", "-p", "BAD_NAME"],
        &["feature", "mv", "auth", "login", "-p", "BAD_NAME"],
        &["add", "t", "-p", "BAD_NAME"],
        &["edit", "1", "-p", "BAD_NAME"],
        &["project", "show", "BAD_NAME"],
        &["project", "edit", "BAD_NAME", "--desc", "x"],
        &["project", "archive", "BAD_NAME"],
        &["project", "rm", "BAD_NAME"],
    ] {
        let output = run(&sandbox, args);
        if output.status.code() != Some(2) {
            failures.push(format!(
                "{args:?}: exit {:?} {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    let output = sandbox
        .tk()
        .args(["--json", "ls"])
        .env("TASKIST_PROJECT", "BAD_NAME")
        .output()
        .expect("run tk");
    if output.status.code() != Some(2) {
        failures.push(format!(
            "TASKIST_PROJECT=BAD_NAME ls: exit {:?}",
            output.status.code()
        ));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
