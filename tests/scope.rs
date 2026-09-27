//! Integration tests for scope resolution, read from `tk ls --json` and `tk feature ls --json`.
#![cfg(test)]

pub mod common;

use std::path::Path;

use common::{Sandbox, err_message, ok_data};
use serde_json::{Value, json};

fn add_project(sandbox: &Sandbox, name: &str, path: Option<&Path>) {
    let mut cmd = sandbox.tk();
    cmd.args(["--json", "project", "add", name]);
    if let Some(path) = path {
        cmd.arg("--path").arg(path);
    }
    ok_data(&cmd.output().expect("run tk"));
}

/// The `scope` of `tk ls --json` run in `dir` with extra arguments and variables.
fn scope_in(sandbox: &Sandbox, dir: &Path, args: &[&str], vars: &[(&str, &str)]) -> Value {
    let mut cmd = sandbox.tk();
    cmd.current_dir(dir).args(["ls", "--json"]).args(args);
    for (name, value) in vars {
        cmd.env(name, value);
    }
    ok_data(&cmd.output().expect("run tk"))["scope"].clone()
}

fn scope(project: Option<&str>, source: &str) -> Value {
    json!({"project": project, "source": source})
}

#[test]
fn flag_wins_then_the_variable_then_the_directory_then_none() {
    let sandbox = Sandbox::new();
    let dir = sandbox.dir("code/c");
    add_project(&sandbox, "a", None);
    add_project(&sandbox, "b", None);
    add_project(&sandbox, "c", Some(&dir));
    let env = [("TASKIST_PROJECT", "b")];

    assert_eq!(
        scope_in(&sandbox, &dir, &["-p", "a"], &env),
        scope(Some("a"), "flag")
    );
    assert_eq!(
        scope_in(&sandbox, &dir, &["--project", "a"], &env),
        scope(Some("a"), "flag")
    );
    assert_eq!(scope_in(&sandbox, &dir, &[], &env), scope(Some("b"), "env"));
    assert_eq!(scope_in(&sandbox, &dir, &[], &[]), scope(Some("c"), "cwd"));
    assert_eq!(
        scope_in(&sandbox, &sandbox.work(), &[], &[]),
        scope(None, "none")
    );
}

#[test]
fn an_empty_variable_counts_as_unset() {
    let sandbox = Sandbox::new();
    let dir = sandbox.dir("code/c");
    add_project(&sandbox, "c", Some(&dir));
    assert_eq!(
        scope_in(&sandbox, &dir, &[], &[("TASKIST_PROJECT", "")]),
        scope(Some("c"), "cwd")
    );
}

#[test]
fn the_longest_nested_project_path_wins() {
    let sandbox = Sandbox::new();
    let outer = sandbox.dir("code");
    let inner = sandbox.dir("code/web");
    add_project(&sandbox, "inner", Some(&inner));
    add_project(&sandbox, "outer", Some(&outer));
    let deep = sandbox.dir("code/web/src/lib");
    assert_eq!(
        scope_in(&sandbox, &deep, &[], &[]),
        scope(Some("inner"), "cwd")
    );
    assert_eq!(
        scope_in(&sandbox, &inner, &[], &[]),
        scope(Some("inner"), "cwd")
    );
    let other = sandbox.dir("code/api");
    assert_eq!(
        scope_in(&sandbox, &other, &[], &[]),
        scope(Some("outer"), "cwd")
    );
}

#[test]
fn a_sibling_sharing_a_name_prefix_does_not_match() {
    let sandbox = Sandbox::new();
    add_project(&sandbox, "web", Some(&sandbox.dir("code/web")));
    let sibling = sandbox.dir("code/webapp");
    assert_eq!(scope_in(&sandbox, &sibling, &[], &[]), scope(None, "none"));
}

#[cfg(unix)]
#[test]
fn a_directory_reached_through_a_symlink_resolves_by_real_path() {
    let sandbox = Sandbox::new();
    let real = sandbox.dir("code/web");
    sandbox.dir("code/web/src");
    add_project(&sandbox, "web", Some(&real));
    let link = sandbox.root().join("link");
    std::os::unix::fs::symlink(&real, &link).expect("create symlink");
    // Control: the link is a different path that leads to the project directory.
    assert_ne!(link, real);
    assert_eq!(
        std::fs::canonicalize(&link).expect("resolve link"),
        std::fs::canonicalize(&real).expect("resolve directory")
    );
    assert_eq!(
        scope_in(&sandbox, &link.join("src"), &[], &[]),
        scope(Some("web"), "cwd")
    );
}

#[test]
fn an_archived_project_never_matches_by_directory() {
    let sandbox = Sandbox::new();
    let outer = sandbox.dir("code");
    let dir = sandbox.dir("code/web");
    add_project(&sandbox, "outer", Some(&outer));
    add_project(&sandbox, "web", Some(&dir));
    assert_eq!(
        scope_in(&sandbox, &dir, &[], &[]),
        scope(Some("web"), "cwd")
    );
    sandbox.ok(&["project", "archive", "web"]);
    assert_eq!(
        scope_in(&sandbox, &dir, &[], &[]),
        scope(Some("outer"), "cwd")
    );
    sandbox.ok(&["project", "archive", "outer"]);
    assert_eq!(scope_in(&sandbox, &dir, &[], &[]), scope(None, "none"));
    // Still addressable by name.
    assert_eq!(
        scope_in(&sandbox, &dir, &["-p", "web"], &[]),
        scope(Some("web"), "flag")
    );
    assert_eq!(
        scope_in(&sandbox, &dir, &[], &[("TASKIST_PROJECT", "web")]),
        scope(Some("web"), "env")
    );
}

#[test]
fn all_projects_ignores_the_variable_and_the_directory() {
    let sandbox = Sandbox::new();
    let dir = sandbox.dir("code/web");
    add_project(&sandbox, "web", Some(&dir));
    add_project(&sandbox, "api", None);
    let env = [("TASKIST_PROJECT", "api")];
    assert_eq!(
        scope_in(&sandbox, &dir, &["--all-projects"], &env),
        scope(None, "none")
    );
    let output = sandbox
        .tk()
        .current_dir(&dir)
        .env("TASKIST_PROJECT", "api")
        .args(["feature", "ls", "--all-projects", "--json"])
        .output()
        .expect("run tk");
    assert_eq!(ok_data(&output)["scope"], scope(None, "none"));
}

#[test]
fn all_projects_with_a_project_flag_is_a_usage_error() {
    let sandbox = Sandbox::new();
    add_project(&sandbox, "web", None);
    for command in [&["ls"][..], &["feature", "ls"][..]] {
        let output = sandbox
            .tk()
            .args(command)
            .args(["--json", "--all-projects", "-p", "web"])
            .output()
            .expect("run tk");
        let message = err_message(&output, 2, "usage");
        assert!(message.contains("--all-projects"), "{message}");
    }
}

#[test]
fn an_unknown_project_by_flag_or_variable_is_not_found() {
    let sandbox = Sandbox::new();
    add_project(&sandbox, "web", None);
    let output = sandbox
        .tk()
        .args(["ls", "--json", "-p", "nope"])
        .output()
        .expect("run tk");
    assert!(err_message(&output, 3, "not_found").contains("nope"));
    let output = sandbox
        .tk()
        .env("TASKIST_PROJECT", "gone")
        .args(["ls", "--json"])
        .output()
        .expect("run tk");
    assert!(err_message(&output, 3, "not_found").contains("gone"));
    sandbox
        .tk()
        .args(["ls", "-p", "nope"])
        .assert()
        .code(3)
        .stdout("")
        .stderr("error: no project named \"nope\"\n");
}

#[test]
fn ls_lists_the_open_tasks_of_the_scope() {
    let sandbox = Sandbox::new();
    add_project(&sandbox, "web", None);
    add_project(&sandbox, "api", None);
    let open = sandbox.seed_task("web", Some("auth"), taskist::model::Status::Doing);
    sandbox.seed_task("web", None, taskist::model::Status::Done);
    let other = sandbox.seed_task("api", None, taskist::model::Status::Todo);

    let data = sandbox.ok(&["ls", "-p", "web"]);
    let tasks = data["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 1, "{data}");
    assert_eq!(tasks[0]["id"], open);
    assert_eq!(tasks[0]["project"], "web");
    assert_eq!(tasks[0]["feature"], "auth");
    assert_eq!(tasks[0]["status"], "doing");
    assert_eq!(tasks[0]["closed_at"], Value::Null);
    assert_eq!(tasks[0]["created_by"], "seed");

    let all = sandbox.ok(&["ls"]);
    let ids: Vec<&Value> = all["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|task| &task["id"])
        .collect();
    // Display order: projects by name.
    assert_eq!(ids, [&json!(other), &json!(open)]);

    sandbox
        .tk()
        .args(["ls", "-p", "web"])
        .assert()
        .code(0)
        .stdout(format!(
            "web  (1 open)\n  auth\n    #{open}  P2  doing    seeded\n"
        ));
}
