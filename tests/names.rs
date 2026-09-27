//! Integration tests for the names given to look up a project or a feature: an invalid name
//! is a `usage` error, a valid name that matches nothing is `not_found`.
#![cfg(test)]

pub mod common;

use common::{Sandbox, err_message};

const INVALID: &str = "BAD_NAME";
const UNKNOWN: &str = "nope";

/// A sandbox with the project `web`, its feature `auth` and one task, whose id is returned.
fn seeded() -> (Sandbox, String) {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);
    let id = sandbox.ok(&["add", "task", "-p", "web", "-f", "auth"])["task"]["id"]
        .as_i64()
        .expect("id");
    (sandbox, id.to_string())
}

/// The commands that take a project name, with `{}` where the name goes.
fn project_commands(id: &str) -> Vec<Vec<String>> {
    let with = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect();
    vec![
        with(&["ls", "-p", "{}"]),
        with(&["next", "-p", "{}"]),
        with(&["find", "task", "-p", "{}"]),
        with(&["feature", "ls", "-p", "{}"]),
        with(&["feature", "mv", "auth", "login", "-p", "{}"]),
        with(&["add", "title", "-p", "{}"]),
        with(&["edit", id, "-p", "{}"]),
        with(&["project", "show", "{}"]),
        with(&["project", "edit", "{}", "--desc", "x"]),
        with(&["project", "archive", "{}"]),
        with(&["project", "rm", "{}", "--force"]),
    ]
}

fn run(sandbox: &Sandbox, args: &[String], name: &str) -> std::process::Output {
    sandbox
        .tk()
        .arg("--json")
        .args(args.iter().map(|arg| arg.replace("{}", name)))
        .output()
        .expect("run tk")
}

#[test]
fn a_known_project_name_is_accepted_by_every_command() {
    // Calibration: each command line runs with the known name, so a failure below comes
    // from the name alone.
    let (sandbox, id) = seeded();
    for args in project_commands(&id) {
        let output = run(&sandbox, &args, "web");
        assert_eq!(output.status.code(), Some(0), "{args:?}: {output:?}");
        if args[0] == "feature" && args[1] == "mv" {
            sandbox.ok(&["feature", "mv", "login", "auth", "-p", "web"]);
        }
        if args[0] == "project" && args[1] == "archive" {
            sandbox.ok(&["project", "archive", "web", "--undo"]);
        }
    }
}

#[test]
fn an_invalid_project_name_is_a_usage_error_and_an_unknown_one_not_found() {
    let (sandbox, id) = seeded();
    for args in project_commands(&id) {
        let message = err_message(&run(&sandbox, &args, INVALID), 2, "usage");
        assert!(message.contains(INVALID), "{args:?}: {message}");
        assert!(message.contains("project name"), "{args:?}: {message}");
        let message = err_message(&run(&sandbox, &args, UNKNOWN), 3, "not_found");
        assert!(message.contains(UNKNOWN), "{args:?}: {message}");
    }
    // Nothing was changed by the refused commands.
    let data = sandbox.ok(&["show", &id]);
    assert_eq!(data["task"]["project"], "web");
    assert_eq!(data["task"]["feature"], "auth");
    assert_eq!(
        sandbox.ok(&["ls", "-p", "web"])["tasks"][0]["id"],
        data["task"]["id"]
    );
}

#[test]
fn an_invalid_project_variable_is_a_usage_error_and_an_unknown_one_not_found() {
    let (sandbox, _) = seeded();
    for args in [&["ls"][..], &["add", "title"][..], &["feature", "ls"][..]] {
        let output = |name: &str| {
            sandbox
                .tk()
                .arg("--json")
                .args(args)
                .env("TASKIST_PROJECT", name)
                .output()
                .expect("run tk")
        };
        // Calibration: the variable is read by this command.
        assert_eq!(output("web").status.code(), Some(0), "{args:?}");
        let message = err_message(&output(INVALID), 2, "usage");
        assert!(message.contains(INVALID), "{args:?}: {message}");
        let message = err_message(&output(UNKNOWN), 3, "not_found");
        assert!(message.contains(UNKNOWN), "{args:?}: {message}");
    }
}

#[test]
fn an_invalid_source_feature_of_feature_mv_is_a_usage_error() {
    let (sandbox, _) = seeded();
    let mv = |old: &str| {
        sandbox
            .tk()
            .args(["--json", "feature", "mv", old, "login", "-p", "web"])
            .output()
            .expect("run tk")
    };
    let message = err_message(&mv(INVALID), 2, "usage");
    assert!(message.contains(INVALID), "{message}");
    assert!(message.contains("feature name"), "{message}");
    let message = err_message(&mv(UNKNOWN), 3, "not_found");
    assert!(message.contains(UNKNOWN), "{message}");
    assert_eq!(mv("auth").status.code(), Some(0));
}
