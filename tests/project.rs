//! Integration tests for the `tk project` commands, in human and JSON output.
#![cfg(test)]

pub mod common;

use common::{Sandbox, err_message, ok_data, real};
use predicates::prelude::*;
use serde_json::{Value, json};
use taskist::model::Status;

fn json_run(sandbox: &Sandbox, args: &[&str]) -> std::process::Output {
    sandbox
        .tk()
        .arg("--json")
        .args(args)
        .output()
        .expect("run tk")
}

fn names(data: &Value) -> Vec<&str> {
    data["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .map(|project| project["name"].as_str().expect("name"))
        .collect()
}

#[test]
fn add_stores_the_canonical_path_and_prints_the_project() {
    let sandbox = Sandbox::new();
    let dir = sandbox.dir("code/web");
    let data = sandbox.ok(&[
        "project",
        "add",
        "web",
        "--path",
        dir.to_str().expect("utf-8"),
        "--desc",
        "the site",
    ]);
    let project = &data["project"];
    assert_eq!(project["name"], "web");
    assert_eq!(project["path"], real(&dir));
    assert_eq!(project["description"], "the site");
    assert_eq!(project["archived"], false);
    assert_eq!(project["open"], 0);
    assert!(
        project["created_at"]
            .as_str()
            .is_some_and(|at| at.ends_with('Z'))
    );
    assert_eq!(
        project.as_object().expect("object").len(),
        6,
        "exactly the documented fields: {project}"
    );

    sandbox
        .tk()
        .args(["project", "add", "api"])
        .assert()
        .code(0)
        .stdout("added project api\n")
        .stderr("");
    assert_eq!(
        sandbox.ok(&["project", "show", "api"])["project"]["path"],
        Value::Null
    );
}

#[test]
fn add_resolves_a_relative_path_and_a_symlink() {
    let sandbox = Sandbox::new();
    sandbox.dir("work/sub");
    let data = sandbox.ok(&["project", "add", "sub", "--path", "sub"]);
    assert_eq!(data["project"]["path"], real(&sandbox.work().join("sub")));
    #[cfg(unix)]
    {
        let target = sandbox.dir("code/web");
        let link = sandbox.root().join("link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        let data = sandbox.ok(&[
            "project",
            "add",
            "web",
            "--path",
            link.to_str().expect("utf-8"),
        ]);
        assert_eq!(data["project"]["path"], real(&target));
        // The same directory through its real path is a duplicate.
        let output = json_run(
            &sandbox,
            &[
                "project",
                "add",
                "www",
                "--path",
                target.to_str().expect("utf-8"),
            ],
        );
        err_message(&output, 4, "conflict");
    }
}

#[test]
fn add_refuses_duplicates_bad_names_and_bad_paths() {
    let sandbox = Sandbox::new();
    let dir = sandbox.dir("code/web");
    let dir_arg = dir.to_str().expect("utf-8");
    sandbox.ok(&["project", "add", "web", "--path", dir_arg]);

    let message = err_message(
        &json_run(&sandbox, &["project", "add", "web"]),
        4,
        "conflict",
    );
    assert!(message.contains("web"), "{message}");
    let message = err_message(
        &json_run(&sandbox, &["project", "add", "www", "--path", dir_arg]),
        4,
        "conflict",
    );
    assert!(message.contains("web"), "{message}");

    for bad in ["Web", "-web", "web_ui", ""] {
        err_message(&json_run(&sandbox, &["project", "add", bad]), 2, "usage");
    }
    let missing = sandbox.root().join("missing");
    err_message(
        &json_run(
            &sandbox,
            &[
                "project",
                "add",
                "gone",
                "--path",
                missing.to_str().expect("utf-8"),
            ],
        ),
        2,
        "usage",
    );
    let file = sandbox.root().join("file");
    std::fs::write(&file, "").expect("write file");
    let message = err_message(
        &json_run(
            &sandbox,
            &[
                "project",
                "add",
                "file",
                "--path",
                file.to_str().expect("utf-8"),
            ],
        ),
        2,
        "usage",
    );
    assert!(message.contains("not a directory"), "{message}");
    sandbox
        .tk()
        .args(["project", "add", "web"])
        .assert()
        .code(4)
        .stdout("")
        .stderr("error: a project named \"web\" already exists\n");
    assert_eq!(names(&sandbox.ok(&["project", "ls"])), ["web"]);
}

#[test]
fn ls_counts_open_tasks_and_all_adds_archived_projects() {
    let sandbox = Sandbox::new();
    for name in ["web", "api", "old"] {
        sandbox.ok(&["project", "add", name]);
    }
    sandbox.seed_task("web", None, Status::Todo);
    sandbox.seed_task("web", Some("auth"), Status::Blocked);
    sandbox.seed_task("web", None, Status::Done);
    sandbox.seed_task("api", None, Status::Dropped);
    sandbox.ok(&["project", "archive", "old"]);

    let data = sandbox.ok(&["project", "ls"]);
    assert_eq!(names(&data), ["api", "web"]);
    assert_eq!(data["projects"][0]["open"], 0);
    assert_eq!(data["projects"][1]["open"], 2);
    let data = sandbox.ok(&["project", "ls", "--all"]);
    assert_eq!(names(&data), ["api", "old", "web"]);
    assert_eq!(data["projects"][1]["archived"], true);

    sandbox
        .tk()
        .args(["project", "ls"])
        .assert()
        .code(0)
        .stdout("api  (0 open)\nweb  (2 open)\n");
    sandbox
        .tk()
        .args(["project", "ls", "--all"])
        .assert()
        .code(0)
        .stdout("api  (0 open)\nold  (0 open)  archived\nweb  (2 open)\n");
}

#[test]
fn show_reports_features_and_per_status_counts() {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web", "--desc", "the site"]);
    sandbox.seed_task("web", Some("auth"), Status::Todo);
    sandbox.seed_task("web", Some("auth"), Status::Done);
    sandbox.seed_task("web", Some("api"), Status::Doing);
    sandbox.seed_task("web", None, Status::Blocked);

    let data = sandbox.ok(&["project", "show", "web"]);
    assert_eq!(data["project"]["name"], "web");
    assert_eq!(data["project"]["open"], 3);
    assert_eq!(
        data["features"],
        json!([
            {"project": "web", "name": "api", "open": 1, "total": 1},
            {"project": "web", "name": "auth", "open": 1, "total": 2},
        ])
    );
    assert_eq!(
        data["counts"],
        json!({"todo": 1, "doing": 1, "blocked": 1, "done": 1, "dropped": 0})
    );

    sandbox
        .tk()
        .args(["project", "show", "web"])
        .assert()
        .code(0)
        .stdout(
            "web  (3 open)\n\
             path: -\n\
             description: the site\n\
             tasks: todo 1, doing 1, blocked 1, done 1, dropped 0\n\
             features:\n  \
             api  1 open / 1 total\n  \
             auth  1 open / 2 total\n",
        );
    err_message(
        &json_run(&sandbox, &["project", "show", "nope"]),
        3,
        "not_found",
    );
}

#[test]
fn edit_renames_changes_the_path_and_the_description() {
    let sandbox = Sandbox::new();
    let first = sandbox.dir("code/one");
    let second = sandbox.dir("code/two");
    sandbox.ok(&[
        "project",
        "add",
        "web",
        "--path",
        first.to_str().expect("utf-8"),
    ]);
    sandbox.ok(&["project", "add", "api"]);

    let data = sandbox.ok(&[
        "project",
        "edit",
        "web",
        "--name",
        "site",
        "--path",
        second.to_str().expect("utf-8"),
        "--desc",
        "new",
    ]);
    assert_eq!(data["project"]["name"], "site");
    assert_eq!(data["project"]["path"], real(&second));
    assert_eq!(data["project"]["description"], "new");

    let data = sandbox.ok(&["project", "edit", "site", "--no-path"]);
    assert_eq!(data["project"]["path"], Value::Null);
    assert_eq!(data["project"]["description"], "new");

    sandbox
        .tk()
        .args(["project", "edit", "site", "--desc", "again"])
        .assert()
        .code(0)
        .stdout("updated project site\n");

    let message = err_message(
        &json_run(&sandbox, &["project", "edit", "site"]),
        2,
        "usage",
    );
    assert!(message.contains("--name"), "{message}");
    err_message(
        &json_run(&sandbox, &["project", "edit", "site", "--name", "api"]),
        4,
        "conflict",
    );
    err_message(
        &json_run(&sandbox, &["project", "edit", "site", "--name", "Bad"]),
        2,
        "usage",
    );
    err_message(
        &json_run(
            &sandbox,
            &["project", "edit", "site", "--path", "x", "--no-path"],
        ),
        2,
        "usage",
    );
    err_message(
        &json_run(&sandbox, &["project", "edit", "nope", "--desc", "x"]),
        3,
        "not_found",
    );
    // Renaming to the current name and keeping the current path is not a conflict.
    let data = sandbox.ok(&["project", "edit", "site", "--name", "site"]);
    assert_eq!(data["project"]["name"], "site");
}

#[test]
fn archive_is_idempotent_and_undo_restores() {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);

    let data = sandbox.ok(&["project", "archive", "web"]);
    assert_eq!(data["project"]["archived"], true);
    assert_eq!(data["changed"], true);
    let data = sandbox.ok(&["project", "archive", "web"]);
    assert_eq!(data["project"]["archived"], true);
    assert_eq!(data["changed"], false);
    assert_eq!(names(&sandbox.ok(&["project", "ls"])), Vec::<&str>::new());

    sandbox
        .tk()
        .args(["project", "archive", "web"])
        .assert()
        .code(0)
        .stdout("project web is already archived\n");
    sandbox
        .tk()
        .args(["project", "archive", "web", "--undo"])
        .assert()
        .code(0)
        .stdout("unarchived project web\n");
    let data = sandbox.ok(&["project", "archive", "web", "--undo"]);
    assert_eq!(data["project"]["archived"], false);
    assert_eq!(data["changed"], false);
    assert_eq!(names(&sandbox.ok(&["project", "ls"])), ["web"]);
    sandbox
        .tk()
        .args(["project", "archive", "web"])
        .assert()
        .code(0)
        .stdout("archived project web\n");
    err_message(
        &json_run(&sandbox, &["project", "archive", "nope"]),
        3,
        "not_found",
    );
}

#[test]
fn rm_refuses_a_project_with_tasks_unless_forced() {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);
    sandbox.ok(&["project", "add", "empty"]);
    let tagged = sandbox.seed_task("web", Some("auth"), Status::Todo);
    sandbox.seed_task("web", None, Status::Done);
    sandbox.ok(&["edit", &tagged.to_string(), "--tag", "ui"]);
    sandbox.note(tagged, taskist::model::NoteKind::Note, "kept until removal");
    let conn = rusqlite::Connection::open(sandbox.db()).expect("open");
    // Control: the tables the forced removal must empty hold rows before it.
    for table in ["task", "feature", "task_tag", "note"] {
        let rows: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count");
        assert!(rows > 0, "{table} is seeded");
    }

    let message = err_message(
        &json_run(&sandbox, &["project", "rm", "web"]),
        4,
        "conflict",
    );
    assert!(message.contains("--force"), "{message}");
    sandbox
        .tk()
        .args(["project", "rm", "web"])
        .assert()
        .code(4)
        .stdout("")
        .stderr(predicate::str::starts_with(
            "error: project web has 2 tasks",
        ));
    assert_eq!(names(&sandbox.ok(&["project", "ls"])), ["empty", "web"]);

    let data = sandbox.ok(&["project", "rm", "web", "--force"]);
    assert_eq!(data, json!({"removed": "web", "tasks": 2}));
    assert_eq!(names(&sandbox.ok(&["project", "ls", "--all"])), ["empty"]);
    let remaining: i64 = sandbox
        .store()
        .read(|tx| Ok(tx.projects(true)?.len()))
        .map(|n| i64::try_from(n).expect("count"))
        .expect("read");
    assert_eq!(remaining, 1);
    for table in ["task", "feature", "task_tag", "note"] {
        let rows: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(rows, 0, "{table}");
    }

    sandbox
        .tk()
        .args(["project", "rm", "empty"])
        .assert()
        .code(0)
        .stdout("removed project empty and 0 tasks\n");
    err_message(
        &json_run(&sandbox, &["project", "rm", "empty"]),
        3,
        "not_found",
    );
}

#[test]
fn global_options_are_accepted_before_or_after_the_subcommand() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .tk()
        .args(["--by", "me", "project", "add", "web", "--json"])
        .output()
        .expect("run tk");
    assert_eq!(ok_data(&output)["project"]["name"], "web");
    let output = sandbox
        .tk()
        .args(["--json", "project", "ls", "--by", "me"])
        .output()
        .expect("run tk");
    assert_eq!(names(&ok_data(&output)), ["web"]);
}
