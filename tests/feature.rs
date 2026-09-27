//! Integration tests for the `tk feature` commands, in human and JSON output.
#![cfg(test)]

pub mod common;

use common::{Sandbox, err_message};
use serde_json::json;
use taskist::model::Status;

fn json_run(sandbox: &Sandbox, args: &[&str]) -> std::process::Output {
    sandbox
        .tk()
        .arg("--json")
        .args(args)
        .output()
        .expect("run tk")
}

/// Two projects: `web` with features `auth` (1 open of 2) and `ui` (0 of 1), `api` with `db`.
fn seeded() -> Sandbox {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);
    sandbox.ok(&["project", "add", "api"]);
    sandbox.seed_task("web", Some("auth"), Status::Todo);
    sandbox.seed_task("web", Some("auth"), Status::Done);
    sandbox.seed_task("web", Some("ui"), Status::Dropped);
    sandbox.seed_task("web", None, Status::Todo);
    sandbox.seed_task("api", Some("db"), Status::Blocked);
    sandbox
}

#[test]
fn ls_reports_open_and_total_counts() {
    let sandbox = seeded();
    let data = sandbox.ok(&["feature", "ls", "-p", "web"]);
    assert_eq!(
        data,
        json!({
            "scope": {"project": "web", "source": "flag"},
            "features": [
                {"project": "web", "name": "auth", "open": 1, "total": 2},
                {"project": "web", "name": "ui", "open": 0, "total": 1},
            ]
        })
    );
    let data = sandbox.ok(&["feature", "ls"]);
    assert_eq!(data["scope"], json!({"project": null, "source": "none"}));
    assert_eq!(
        data["features"],
        json!([
            {"project": "api", "name": "db", "open": 1, "total": 1},
            {"project": "web", "name": "auth", "open": 1, "total": 2},
            {"project": "web", "name": "ui", "open": 0, "total": 1},
        ])
    );

    sandbox
        .tk()
        .args(["feature", "ls", "-p", "web"])
        .assert()
        .code(0)
        .stdout("web\n  auth  1 open / 2 total\n  ui  0 open / 1 total\n");
    sandbox
        .tk()
        .args(["feature", "ls"])
        .assert()
        .code(0)
        .stdout(
            "api\n  db  1 open / 1 total\nweb\n  auth  1 open / 2 total\n  ui  0 open / 1 total\n",
        );
}

#[test]
fn ls_hides_archived_projects_unless_named() {
    let sandbox = seeded();
    sandbox.ok(&["project", "archive", "api"]);
    let data = sandbox.ok(&["feature", "ls"]);
    assert_eq!(data["features"].as_array().expect("features").len(), 2);
    let data = sandbox.ok(&["feature", "ls", "-p", "api"]);
    assert_eq!(
        data["features"],
        json!([{"project": "api", "name": "db", "open": 1, "total": 1}])
    );
    sandbox.ok(&["project", "add", "empty"]);
    sandbox
        .tk()
        .args(["feature", "ls", "-p", "empty"])
        .assert()
        .code(0)
        .stdout("empty\n  no features\n");
}

#[test]
fn mv_renames_when_the_target_does_not_exist() {
    let sandbox = seeded();
    let data = sandbox.ok(&["feature", "mv", "auth", "login", "-p", "web"]);
    assert_eq!(
        data,
        json!({
            "feature": {"project": "web", "name": "login", "open": 1, "total": 2},
            "merged": false,
            "moved": 2
        })
    );
    let names: Vec<String> = sandbox.ok(&["feature", "ls", "-p", "web"])["features"]
        .as_array()
        .expect("features")
        .iter()
        .map(|feature| feature["name"].as_str().expect("name").to_owned())
        .collect();
    assert_eq!(names, ["login", "ui"]);
    sandbox
        .tk()
        .args(["feature", "mv", "login", "auth", "-p", "web"])
        .assert()
        .code(0)
        .stdout("renamed feature login to auth in web (2 tasks)\n");
}

#[test]
fn mv_merges_into_an_existing_feature() {
    let sandbox = seeded();
    let data = sandbox.ok(&["feature", "mv", "ui", "auth", "-p", "web"]);
    assert_eq!(
        data,
        json!({
            "feature": {"project": "web", "name": "auth", "open": 1, "total": 3},
            "merged": true,
            "moved": 1
        })
    );
    assert_eq!(
        sandbox.ok(&["feature", "ls", "-p", "web"])["features"],
        json!([{"project": "web", "name": "auth", "open": 1, "total": 3}])
    );
    sandbox.seed_task("web", Some("ui"), Status::Todo);
    sandbox.seed_task("web", Some("ui"), Status::Todo);
    sandbox
        .tk()
        .args(["feature", "mv", "ui", "auth", "-p", "web"])
        .assert()
        .code(0)
        .stdout("merged feature ui into auth in web (2 tasks moved)\n");
}

#[test]
fn mv_refuses_unknown_sources_bad_names_and_a_missing_scope() {
    let sandbox = seeded();
    let message = err_message(
        &json_run(&sandbox, &["feature", "mv", "nope", "x", "-p", "web"]),
        3,
        "not_found",
    );
    assert!(message.contains("nope"), "{message}");
    // A feature of another project is unknown here.
    err_message(
        &json_run(&sandbox, &["feature", "mv", "db", "x", "-p", "web"]),
        3,
        "not_found",
    );
    err_message(
        &json_run(&sandbox, &["feature", "mv", "auth", "Bad", "-p", "web"]),
        2,
        "usage",
    );
    err_message(
        &json_run(&sandbox, &["feature", "mv", "auth", "auth", "-p", "web"]),
        2,
        "usage",
    );
    let message = err_message(
        &json_run(&sandbox, &["feature", "mv", "auth", "x"]),
        2,
        "usage",
    );
    assert!(message.contains("-p/--project"), "{message}");
    err_message(
        &json_run(&sandbox, &["feature", "mv", "auth", "x", "-p", "nope"]),
        3,
        "not_found",
    );
}

#[test]
fn mv_uses_the_directory_scope() {
    let sandbox = Sandbox::new();
    let dir = sandbox.dir("code/web");
    sandbox.ok(&[
        "project",
        "add",
        "web",
        "--path",
        dir.to_str().expect("utf-8"),
    ]);
    sandbox.seed_task("web", Some("auth"), Status::Todo);
    let output = sandbox
        .tk()
        .current_dir(&dir)
        .args(["--json", "feature", "mv", "auth", "login"])
        .output()
        .expect("run tk");
    assert_eq!(common::ok_data(&output)["feature"]["name"], "login");
}
