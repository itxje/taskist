//! Integration tests for the task commands `add`, `ls`, `show`, `edit`, `next` and `find`,
//! in human and JSON output.
#![cfg(test)]

pub mod common;

use std::collections::BTreeSet;

use common::{Sandbox, err_message, ok_data};
use serde_json::{Value, json};
use taskist::model::{NoteKind, Status};

const TASK_KEYS: [&str; 12] = [
    "id",
    "project",
    "feature",
    "title",
    "body",
    "status",
    "priority",
    "tags",
    "created_at",
    "updated_at",
    "closed_at",
    "created_by",
];

fn keys(value: &Value) -> BTreeSet<&str> {
    assert!(value.is_object(), "object expected: {value}");
    value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect()
}

fn set(names: &[&'static str]) -> BTreeSet<&'static str> {
    names.iter().copied().collect()
}

fn assert_task_shape(task: &Value) {
    assert_eq!(keys(task), set(&TASK_KEYS), "{task}");
}

fn json_run(sandbox: &Sandbox, args: &[&str]) -> std::process::Output {
    sandbox
        .tk()
        .arg("--json")
        .args(args)
        .output()
        .expect("run tk")
}

fn project(sandbox: &Sandbox, name: &str) {
    sandbox.ok(&["project", "add", name]);
}

/// Runs `tk add` with `args` and returns the new task id.
fn add(sandbox: &Sandbox, args: &[&str]) -> i64 {
    let data = sandbox.ok(&[&["add"][..], args].concat());
    assert_eq!(keys(&data), set(&["task"]), "{data}");
    data["task"]["id"].as_i64().expect("id")
}

fn show(sandbox: &Sandbox, id: i64) -> Value {
    sandbox.ok(&["show", &id.to_string()])["task"].clone()
}

fn ids(tasks: &Value) -> Vec<i64> {
    tasks
        .as_array()
        .expect("tasks")
        .iter()
        .map(|task| task["id"].as_i64().expect("id"))
        .collect()
}

fn human(sandbox: &Sandbox, args: &[&str]) -> String {
    let output = sandbox.tk().args(args).output().expect("run tk");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    String::from_utf8(output.stdout).expect("utf-8")
}

// ---------------------------------------------------------------- add

#[test]
fn add_creates_a_todo_task_in_the_task_shape() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let data = sandbox.ok(&["add", "  Fix the login  ", "-p", "web"]);
    assert_eq!(keys(&data), set(&["task"]));
    let task = &data["task"];
    assert_task_shape(task);
    assert_eq!(task["project"], "web");
    assert_eq!(task["feature"], Value::Null);
    assert_eq!(task["title"], "Fix the login");
    assert_eq!(task["body"], "");
    assert_eq!(task["status"], "todo");
    assert_eq!(task["priority"], 2);
    assert_eq!(task["tags"], json!([]));
    assert_eq!(task["closed_at"], Value::Null);
    assert_eq!(task["created_at"], task["updated_at"]);

    let data = sandbox.ok(&[
        "add", "Second", "-p", "web", "--pri", "0", "--tag", "ui", "--tag", "bug", "--body",
        "details",
    ]);
    assert_eq!(data["task"]["priority"], 0);
    assert_eq!(data["task"]["tags"], json!(["bug", "ui"]));
    assert_eq!(data["task"]["body"], "details");

    let text = human(&sandbox, &["add", "Third", "-p", "web", "-f", "auth"]);
    let id = data["task"]["id"].as_i64().expect("id") + 1;
    assert_eq!(text, format!("added #{id} to web/auth: Third\n"));
}

#[test]
fn add_creates_a_missing_feature_once_and_reuses_it() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let first = add(&sandbox, &["one", "-p", "web", "-f", "auth"]);
    let second = add(&sandbox, &["two", "-p", "web", "-f", "auth"]);
    assert_eq!(show(&sandbox, first)["feature"], "auth");
    assert_eq!(show(&sandbox, second)["feature"], "auth");
    let features = sandbox.ok(&["feature", "ls", "-p", "web"])["features"].clone();
    assert_eq!(
        features,
        json!([{"project": "web", "name": "auth", "open": 2, "total": 2}])
    );
}

#[test]
fn add_reads_the_body_from_stdin_for_a_dash() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let body = "first line\n-second line with a dash\n\ttab\n";
    let output = sandbox
        .tk()
        .args(["--json", "add", "With body", "-p", "web", "--body", "-"])
        .write_stdin(body)
        .output()
        .expect("run tk");
    let id = ok_data(&output)["task"]["id"].as_i64().expect("id");
    assert_eq!(show(&sandbox, id)["body"], body);
}

#[test]
fn add_refuses_an_archived_project_bad_values_and_a_missing_scope() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    project(&sandbox, "old");
    sandbox.ok(&["project", "archive", "old"]);
    let message = err_message(
        &json_run(&sandbox, &["add", "x", "-p", "old"]),
        4,
        "conflict",
    );
    assert!(message.contains("archived"), "{message}");

    let message = err_message(&json_run(&sandbox, &["add", "x"]), 2, "usage");
    assert!(message.contains("-p/--project"), "{message}");
    sandbox
        .tk()
        .args(["add", "x"])
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicates::str::contains("-p/--project"));

    for args in [
        &["add", "x", "-p", "web", "--pri", "4"][..],
        &["add", "x", "-p", "web", "--tag", "Bad"][..],
        &["add", "x", "-p", "web", "-f", "no_underscores"][..],
        &["add", "   ", "-p", "web"][..],
        &["add", "one\ntwo", "-p", "web"][..],
    ] {
        err_message(&json_run(&sandbox, args), 2, "usage");
    }
    err_message(
        &json_run(&sandbox, &["add", "x", "-p", "nope"]),
        3,
        "not_found",
    );
    assert_eq!(sandbox.ok(&["ls", "--all-projects"])["tasks"], json!([]));
}

#[test]
fn add_takes_the_project_from_the_variable_and_the_directory() {
    let sandbox = Sandbox::new();
    let dir = sandbox.dir("code/web");
    sandbox.ok(&[
        "project",
        "add",
        "web",
        "--path",
        dir.to_str().expect("utf-8"),
    ]);
    project(&sandbox, "api");
    let output = sandbox
        .tk()
        .current_dir(&dir)
        .args(["--json", "add", "from cwd"])
        .output()
        .expect("run tk");
    assert_eq!(ok_data(&output)["task"]["project"], "web");
    let output = sandbox
        .tk()
        .current_dir(&dir)
        .env("TASKIST_PROJECT", "api")
        .args(["--json", "add", "from env"])
        .output()
        .expect("run tk");
    assert_eq!(ok_data(&output)["task"]["project"], "api");
}

#[test]
fn created_by_follows_the_flag_then_the_variables_then_unknown() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let created_by = |cmd: &mut assert_cmd::Command| {
        let output = cmd
            .args(["--json", "add", "t", "-p", "web"])
            .output()
            .expect("run tk");
        let id = ok_data(&output)["task"]["id"].as_i64().expect("id");
        show(&sandbox, id)["created_by"].clone()
    };
    assert_eq!(
        created_by(
            sandbox
                .tk()
                .env("TASKIST_ACTOR", "agent:web")
                .env("USER", "alice")
                .args(["--by", "cli-actor"])
        ),
        "cli-actor"
    );
    assert_eq!(
        created_by(
            sandbox
                .tk()
                .env("TASKIST_ACTOR", "agent:web")
                .env("USER", "alice")
        ),
        "agent:web"
    );
    assert_eq!(created_by(sandbox.tk().env("USER", "alice")), "alice");
    assert_eq!(created_by(&mut sandbox.tk()), "unknown");
    // A value beginning with '-' is the actor, not an option.
    assert_eq!(created_by(sandbox.tk().args(["--by", "-bot"])), "-bot");
}

// ---------------------------------------------------------------- show

#[test]
fn show_prints_the_task_and_its_notes_in_order() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let id = add(
        &sandbox,
        &[
            "Paginate", "-p", "web", "-f", "api", "--tag", "x", "--body", "the body",
        ],
    );
    sandbox.set_status(id, Status::Blocked);
    sandbox.note(id, NoteKind::Blocked, "waiting on schema");
    sandbox.note(id, NoteKind::Note, "second");

    let data = sandbox.ok(&["show", &id.to_string()]);
    assert_eq!(keys(&data), set(&["task", "notes"]));
    assert_task_shape(&data["task"]);
    assert_eq!(data["task"]["status"], "blocked");
    let notes = data["notes"].as_array().expect("notes");
    assert_eq!(notes.len(), 2);
    for note in notes {
        assert_eq!(
            keys(note),
            set(&["id", "kind", "text", "author", "created_at"])
        );
    }
    assert_eq!(notes[0]["kind"], "blocked");
    assert_eq!(notes[0]["text"], "waiting on schema");
    assert_eq!(notes[0]["author"], "seed");
    assert_eq!(notes[1]["text"], "second");

    let text = human(&sandbox, &["show", &id.to_string()]);
    let task = &data["task"];
    let created = task["created_at"].as_str().expect("created_at");
    let updated = task["updated_at"].as_str().expect("updated_at");
    let note_times: Vec<&str> = notes
        .iter()
        .map(|note| note["created_at"].as_str().expect("created_at"))
        .collect();
    assert_eq!(
        text,
        format!(
            "#{id}  Paginate\n\
             project:  web\n\
             feature:  api\n\
             status:   blocked\n\
             priority: P2\n\
             tags:     x\n\
             created:  {created} by unknown\n\
             updated:  {updated}\n\
             \n\
             the body\n\
             \n\
             notes:\n\
             \x20 {}  blocked  seed: waiting on schema\n\
             \x20 {}  note  seed: second\n",
            note_times[0], note_times[1]
        )
    );
}

#[test]
fn show_of_an_unknown_id_is_not_found() {
    let sandbox = Sandbox::new();
    let message = err_message(&json_run(&sandbox, &["show", "42"]), 3, "not_found");
    assert!(message.contains("42"), "{message}");
    err_message(&json_run(&sandbox, &["show", "abc"]), 2, "usage");
}

// ---------------------------------------------------------------- ls

/// A project `web` with tasks in two features and none, one of them blocked and one
/// done, and a project `cli` with one task; returns the ids in creation order.
fn grouped_fixture(sandbox: &Sandbox) -> [i64; 6] {
    project(sandbox, "web");
    project(sandbox, "cli");
    let sso = add(sandbox, &["Add SSO login", "-p", "web", "-f", "auth"]);
    let race = add(
        sandbox,
        &[
            "Fix token refresh race",
            "-p",
            "web",
            "-f",
            "auth",
            "--pri",
            "1",
        ],
    );
    let orders = add(sandbox, &["Paginate /orders", "-p", "web", "-f", "api"]);
    let readme = add(sandbox, &["Clean up README", "-p", "web", "--pri", "3"]);
    let shipped = add(sandbox, &["Shipped", "-p", "web", "-f", "auth"]);
    let flags = add(sandbox, &["Parse flags", "-p", "cli", "--tag", "x"]);
    sandbox.set_status(race, Status::Doing);
    sandbox.set_status(orders, Status::Blocked);
    sandbox.note(orders, NoteKind::Blocked, "old reason");
    sandbox.note(orders, NoteKind::Note, "not a reason");
    sandbox.note(orders, NoteKind::Blocked, "waiting on schema");
    sandbox.set_status(shipped, Status::Done);
    [sso, race, orders, readme, shipped, flags]
}

#[test]
fn ls_groups_by_project_and_feature_in_the_documented_layout() {
    let sandbox = Sandbox::new();
    let [sso, race, orders, readme, _, flags] = grouped_fixture(&sandbox);
    let web = format!(
        "web  (4 open)\n\
         \x20 api\n\
         \x20   #{orders}  P2  blocked  Paginate /orders  (waiting on schema)\n\
         \x20 auth\n\
         \x20   #{race}  P1  doing    Fix token refresh race\n\
         \x20   #{sso}  P2  todo     Add SSO login\n\
         \x20 -\n\
         \x20   #{readme}  P3  todo     Clean up README\n"
    );
    assert_eq!(human(&sandbox, &["ls", "-p", "web"]), web);
    let cli = format!("cli  (1 open)\n  -\n    #{flags}  P2  todo     Parse flags\n");
    assert_eq!(human(&sandbox, &["ls"]), format!("{cli}{web}"));

    // JSON lists the same tasks in the same order.
    let data = sandbox.ok(&["ls"]);
    assert_eq!(keys(&data), set(&["scope", "tasks"]));
    assert_eq!(data["scope"], json!({"project": null, "source": "none"}));
    for task in data["tasks"].as_array().expect("tasks") {
        assert_task_shape(task);
    }
    assert_eq!(ids(&data["tasks"]), [flags, orders, race, sso, readme]);
}

#[test]
fn ls_of_an_empty_scope() {
    let sandbox = Sandbox::new();
    assert_eq!(human(&sandbox, &["ls"]), "no tasks\n");
    project(&sandbox, "web");
    assert_eq!(human(&sandbox, &["ls", "-p", "web"]), "web  (0 open)\n");
}

#[test]
fn ls_filters_by_feature_status_tag_all_and_limit() {
    let sandbox = Sandbox::new();
    let [sso, race, orders, readme, shipped, flags] = grouped_fixture(&sandbox);
    let tagged = add(
        &sandbox,
        &["Tagged", "-p", "web", "--tag", "x", "--tag", "y"],
    );
    let dropped = add(&sandbox, &["Dropped", "-p", "web", "-f", "auth"]);
    sandbox.set_status(dropped, Status::Dropped);
    let list = |args: &[&str]| ids(&sandbox.ok(&[&["ls"][..], args].concat())["tasks"]);

    assert_eq!(list(&["-p", "web", "-f", "auth"]), [race, sso]);
    assert_eq!(
        list(&["-p", "web", "--status", "todo,blocked"]),
        [orders, sso, tagged, readme]
    );
    assert_eq!(
        list(&["-p", "web", "--status", "done", "--status", "dropped"]),
        [shipped, dropped]
    );
    assert_eq!(list(&["--tag", "x"]), [flags, tagged]);
    assert_eq!(list(&["-p", "web", "--tag", "y"]), [tagged]);
    assert_eq!(
        list(&["-p", "web", "-f", "auth", "--all"]),
        [race, sso, shipped, dropped]
    );
    assert_eq!(list(&["-p", "web", "--limit", "2"]), [orders, race]);
    assert_eq!(list(&["--limit", "0"]), Vec::<i64>::new());
    assert_eq!(
        list(&["--all-projects", "--limit", "3"]),
        [flags, orders, race]
    );
    let output = sandbox
        .tk()
        .env("TASKIST_PROJECT", "web")
        .args(["--json", "ls", "--all-projects"])
        .output()
        .expect("run tk");
    let data = ok_data(&output);
    assert_eq!(data["scope"], json!({"project": null, "source": "none"}));
    assert_eq!(ids(&data["tasks"])[0], flags);

    assert_eq!(
        human(&sandbox, &["ls", "-p", "web", "--limit", "1"]),
        format!(
            "web  (5 open)\n  api\n    #{orders}  P2  blocked  Paginate /orders  (waiting on schema)\n"
        )
    );

    err_message(
        &json_run(&sandbox, &["ls", "--status", "later"]),
        2,
        "usage",
    );
    err_message(
        &json_run(&sandbox, &["ls", "--status", "todo", "--all"]),
        2,
        "usage",
    );
    let message = err_message(&json_run(&sandbox, &["ls", "-f", "nope"]), 3, "not_found");
    assert!(message.contains("nope"), "{message}");
}

// ---------------------------------------------------------------- edit

fn edit(sandbox: &Sandbox, id: i64, args: &[&str]) -> Value {
    let data = sandbox.ok(&[&["edit", &id.to_string()][..], args].concat());
    assert_eq!(keys(&data), set(&["task"]), "{data}");
    assert_task_shape(&data["task"]);
    data["task"].clone()
}

#[test]
fn edit_changes_each_field_alone() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let id = add(
        &sandbox,
        &["Old", "-p", "web", "-f", "auth", "--body", "old body"],
    );
    let before = show(&sandbox, id);

    let task = edit(&sandbox, id, &["--title", "  New title "]);
    assert_eq!(task["title"], "New title");
    assert_eq!(task["body"], "old body");
    assert!(task["updated_at"].as_str() >= before["updated_at"].as_str());

    let task = edit(&sandbox, id, &["--body", "-starts with a dash"]);
    assert_eq!(task["body"], "-starts with a dash");
    assert_eq!(task["title"], "New title");

    let output = sandbox
        .tk()
        .args(["--json", "edit", &id.to_string(), "--body", "-"])
        .write_stdin("from stdin\n")
        .output()
        .expect("run tk");
    assert_eq!(ok_data(&output)["task"]["body"], "from stdin\n");

    let task = edit(&sandbox, id, &["--pri", "0"]);
    assert_eq!(task["priority"], 0);
    assert_eq!(task["feature"], "auth");

    let task = edit(&sandbox, id, &["-f", "api"]);
    assert_eq!(task["feature"], "api");
    let features: Vec<String> = sandbox.ok(&["feature", "ls", "-p", "web"])["features"]
        .as_array()
        .expect("features")
        .iter()
        .map(|feature| feature["name"].as_str().expect("name").to_owned())
        .collect();
    assert_eq!(features, ["api", "auth"]);

    let task = edit(&sandbox, id, &["--no-feature"]);
    assert_eq!(task["feature"], Value::Null);

    let after = show(&sandbox, id);
    assert_eq!(after["title"], "New title");
    assert_eq!(after["priority"], 0);
    assert_eq!(after["created_at"], before["created_at"]);
    assert_eq!(after["created_by"], before["created_by"]);

    assert_eq!(
        human(&sandbox, &["edit", &id.to_string(), "--pri", "1"]),
        format!("updated #{id} in web: New title\n")
    );
}

#[test]
fn edit_adds_and_removes_tags_including_values_starting_with_a_dash() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let id = add(&sandbox, &["t", "-p", "web", "--tag", "old"]);
    let task = edit(&sandbox, id, &["--tag", "+new", "--tag", "bare"]);
    assert_eq!(task["tags"], json!(["bare", "new", "old"]));
    let task = edit(&sandbox, id, &["--tag", "-old"]);
    assert_eq!(task["tags"], json!(["bare", "new"]));
    let task = edit(
        &sandbox,
        id,
        &["--tag=-bare", "--tag", "-new", "--tag", "+z"],
    );
    assert_eq!(task["tags"], json!(["z"]));
    // Removing a tag the task does not have changes nothing.
    let task = edit(&sandbox, id, &["--tag", "-absent"]);
    assert_eq!(task["tags"], json!(["z"]));
    for bad in ["+", "-", "+Bad", "-a_b"] {
        err_message(
            &json_run(&sandbox, &["edit", &id.to_string(), "--tag", bad]),
            2,
            "usage",
        );
    }
    assert_eq!(show(&sandbox, id)["tags"], json!(["z"]));
}

#[test]
fn edit_moves_a_task_to_another_project_with_its_feature() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    project(&sandbox, "api");
    let id = add(&sandbox, &["Move me", "-p", "web", "-f", "auth"]);
    let other = add(&sandbox, &["Stay", "-p", "web", "-f", "auth"]);

    let task = edit(&sandbox, id, &["-p", "api"]);
    assert_eq!(task["project"], "api");
    assert_eq!(task["feature"], "auth");
    let api_features = sandbox.ok(&["feature", "ls", "-p", "api"])["features"].clone();
    assert_eq!(
        api_features,
        json!([{"project": "api", "name": "auth", "open": 1, "total": 1}])
    );
    assert_eq!(show(&sandbox, other)["project"], "web");
    assert_eq!(show(&sandbox, other)["feature"], "auth");

    // Back into a project where the feature exists: it is reused, not duplicated.
    let task = edit(&sandbox, id, &["--project", "web"]);
    assert_eq!(task["feature"], "auth");
    let web_features = sandbox.ok(&["feature", "ls", "-p", "web"])["features"].clone();
    assert_eq!(
        web_features,
        json!([{"project": "web", "name": "auth", "open": 2, "total": 2}])
    );

    // A move together with a feature change puts the task in that feature of the target.
    let task = edit(&sandbox, id, &["-p", "api", "-f", "db"]);
    assert_eq!(
        (&task["project"], &task["feature"]),
        (&json!("api"), &json!("db"))
    );
    let task = edit(&sandbox, id, &["-p", "web", "--no-feature"]);
    assert_eq!(
        (&task["project"], &task["feature"]),
        (&json!("web"), &Value::Null)
    );

    project(&sandbox, "old");
    sandbox.ok(&["project", "archive", "old"]);
    err_message(
        &json_run(&sandbox, &["edit", &id.to_string(), "-p", "old"]),
        4,
        "conflict",
    );
    err_message(
        &json_run(&sandbox, &["edit", &id.to_string(), "-p", "nope"]),
        3,
        "not_found",
    );
    assert_eq!(show(&sandbox, id)["project"], "web");
}

#[test]
fn edit_needs_an_option_and_a_known_task() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let id = add(&sandbox, &["t", "-p", "web"]);
    let message = err_message(&json_run(&sandbox, &["edit", &id.to_string()]), 2, "usage");
    assert!(message.contains("--title"), "{message}");
    err_message(
        &json_run(
            &sandbox,
            &["edit", &id.to_string(), "-f", "a", "--no-feature"],
        ),
        2,
        "usage",
    );
    err_message(
        &json_run(&sandbox, &["edit", &id.to_string(), "--title", " "]),
        2,
        "usage",
    );
    err_message(
        &json_run(&sandbox, &["edit", "999", "--pri", "1"]),
        3,
        "not_found",
    );
}

// ---------------------------------------------------------------- next

fn next(sandbox: &Sandbox, args: &[&str]) -> Value {
    let data = sandbox.ok(&[&["next"][..], args].concat());
    assert_eq!(keys(&data), set(&["scope", "task"]), "{data}");
    data
}

fn next_id(sandbox: &Sandbox, args: &[&str]) -> Option<i64> {
    next(sandbox, args)["task"]["id"].as_i64()
}

#[test]
fn next_orders_by_priority_then_doing_then_age_then_id() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let old = "2026-01-01T00:00:00.000Z";
    let new = "2026-06-01T00:00:00.000Z";
    let todo_old = add(&sandbox, &["todo old", "-p", "web"]);
    let doing_new = add(&sandbox, &["doing new", "-p", "web"]);
    let todo_new = add(&sandbox, &["todo new", "-p", "web"]);
    let todo_new_later_id = add(&sandbox, &["todo new, later id", "-p", "web"]);
    let blocked = add(&sandbox, &["blocked", "-p", "web", "--pri", "0"]);
    let done = add(&sandbox, &["done", "-p", "web", "--pri", "0"]);
    sandbox.set_status(doing_new, Status::Doing);
    sandbox.set_status(blocked, Status::Blocked);
    sandbox.set_status(done, Status::Done);
    for (id, at) in [
        (todo_old, old),
        (doing_new, new),
        (todo_new, new),
        (todo_new_later_id, new),
    ] {
        sandbox.sql(
            "UPDATE task SET created_at = ?2 WHERE id = ?1",
            rusqlite::params![id, at],
        );
    }
    // Calibration: the ages were set, so age and id order disagree for todo_old.
    assert_eq!(show(&sandbox, todo_old)["created_at"], old);
    assert!(todo_old < doing_new);

    // doing before todo at equal priority, although the todo task is older.
    assert_eq!(next_id(&sandbox, &["-p", "web"]), Some(doing_new));
    // Priority first: a P1 todo beats the P2 doing task.
    edit(&sandbox, todo_new_later_id, &["--pri", "1"]);
    assert_eq!(next_id(&sandbox, &["-p", "web"]), Some(todo_new_later_id));
    edit(&sandbox, todo_new_later_id, &["--pri", "3"]);
    // Then age: with the doing task gone, the older todo wins over the lower id.
    sandbox.set_status(doing_new, Status::Done);
    sandbox.sql(
        "UPDATE task SET created_at = ?2 WHERE id = ?1",
        rusqlite::params![todo_old, "2026-12-01T00:00:00.000Z"],
    );
    assert_eq!(next_id(&sandbox, &["-p", "web"]), Some(todo_new));
    // Then id: equal priority, status and age.
    sandbox.sql(
        "UPDATE task SET created_at = ?1 WHERE id IN (?2, ?3)",
        rusqlite::params![new, todo_old, todo_new],
    );
    assert_eq!(next_id(&sandbox, &["-p", "web"]), Some(todo_old));

    let data = next(&sandbox, &["-p", "web"]);
    assert_eq!(data["scope"], json!({"project": "web", "source": "flag"}));
    assert_task_shape(&data["task"]);
    assert_eq!(
        human(&sandbox, &["next", "-p", "web"]),
        format!("#{todo_old}  P2  todo     todo old  (web)\n")
    );
}

#[test]
fn next_filters_by_feature_and_is_null_on_an_empty_scope() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    project(&sandbox, "api");
    let data = next(&sandbox, &["-p", "web"]);
    assert_eq!(data["task"], Value::Null);
    assert_eq!(human(&sandbox, &["next", "-p", "web"]), "no open task\n");

    let plain = add(&sandbox, &["plain", "-p", "web", "--pri", "0"]);
    let auth = add(&sandbox, &["in auth", "-p", "web", "-f", "auth"]);
    let api = add(&sandbox, &["in api", "-p", "api", "--pri", "1"]);
    assert_eq!(next_id(&sandbox, &["-p", "web"]), Some(plain));
    assert_eq!(next_id(&sandbox, &["-p", "web", "-f", "auth"]), Some(auth));
    assert_eq!(next_id(&sandbox, &["-p", "api"]), Some(api));
    assert_eq!(next_id(&sandbox, &["--all-projects"]), Some(plain));
    sandbox.set_status(auth, Status::Done);
    assert_eq!(next_id(&sandbox, &["-p", "web", "-f", "auth"]), None);
    err_message(
        &json_run(&sandbox, &["next", "-p", "web", "-f", "nope"]),
        3,
        "not_found",
    );
}

// ---------------------------------------------------------------- find

fn find(sandbox: &Sandbox, args: &[&str]) -> Vec<i64> {
    let data = sandbox.ok(&[&["find"][..], args].concat());
    assert_eq!(keys(&data), set(&["scope", "tasks"]), "{data}");
    for task in data["tasks"].as_array().expect("tasks") {
        assert_task_shape(task);
    }
    ids(&data["tasks"])
}

#[test]
fn find_matches_title_body_and_notes_case_insensitively() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    project(&sandbox, "api");
    let title = add(&sandbox, &["Fix the Token refresh", "-p", "web"]);
    let body = add(
        &sandbox,
        &["Other", "-p", "web", "--body", "the TOKEN expires"],
    );
    let note = add(&sandbox, &["Third", "-p", "api"]);
    sandbox.note(note, NoteKind::Note, "token was rotated");
    let unrelated = add(&sandbox, &["Unrelated", "-p", "web"]);
    let accented = add(&sandbox, &["ÉCOLE notes", "-p", "web"]);

    assert_eq!(find(&sandbox, &["token"]), [note, title, body]);
    assert_eq!(find(&sandbox, &["tOkEn", "-p", "web"]), [title, body]);
    assert_eq!(find(&sandbox, &["école"]), [accented]);
    assert_eq!(find(&sandbox, &["unrel"]), [unrelated]);
    assert!(find(&sandbox, &["absent"]).is_empty());

    let data = sandbox.ok(&["find", "token", "-p", "api"]);
    assert_eq!(data["scope"], json!({"project": "api", "source": "flag"}));
    assert_eq!(
        human(&sandbox, &["find", "token", "-p", "api"]),
        format!("api  (1 open)\n  -\n    #{note}  P2  todo     Third\n")
    );
}

#[test]
fn find_treats_wildcards_and_the_escape_character_literally() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let percent = add(&sandbox, &["100% done", "-p", "web"]);
    let plain = add(&sandbox, &["100 done", "-p", "web"]);
    let underscore = add(&sandbox, &["snake_case", "-p", "web"]);
    let other = add(&sandbox, &["snakeXcase", "-p", "web"]);
    let backslash = add(&sandbox, &[r"C:\temp", "-p", "web"]);
    let escaped = add(&sandbox, &[r"a\%b", "-p", "web"]);
    // Calibration: without wildcards both spellings match.
    assert_eq!(find(&sandbox, &["done"]), [percent, plain]);
    assert_eq!(find(&sandbox, &["snake"]), [underscore, other]);
    assert_eq!(find(&sandbox, &["100%"]), [percent]);
    assert_eq!(find(&sandbox, &["%"]), [percent, escaped]);
    assert_eq!(find(&sandbox, &["snake_"]), [underscore]);
    assert_eq!(find(&sandbox, &["_"]), [underscore]);
    assert_eq!(find(&sandbox, &[r"\"]), [backslash, escaped]);
    assert_eq!(find(&sandbox, &[r"\%"]), [escaped]);
    assert_eq!(find(&sandbox, &[r"\t"]), [backslash]);
}

#[test]
fn find_hides_closed_tasks_unless_all() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let open = add(&sandbox, &["bug one", "-p", "web"]);
    let done = add(&sandbox, &["bug two", "-p", "web"]);
    let dropped = add(&sandbox, &["bug three", "-p", "web"]);
    sandbox.set_status(done, Status::Done);
    sandbox.set_status(dropped, Status::Dropped);
    assert_eq!(find(&sandbox, &["bug"]), [open]);
    assert_eq!(find(&sandbox, &["bug", "--all"]), [open, done, dropped]);
}

// ---------------------------------------------------------------- colour

fn has_escape(bytes: &[u8]) -> bool {
    bytes.contains(&0x1b)
}

/// Every read command in human and JSON form, run with `vars`.
fn outputs(
    sandbox: &Sandbox,
    id: i64,
    vars: &[(&str, &str)],
) -> Vec<(String, std::process::Output)> {
    let id = id.to_string();
    let mut runs = Vec::new();
    for args in [
        vec!["ls", "-p", "web"],
        vec!["show", &id],
        vec!["next", "-p", "web"],
        vec!["find", "task", "-p", "web"],
    ] {
        for json in [false, true] {
            let mut cmd = sandbox.tk();
            cmd.args(&args);
            if json {
                cmd.arg("--json");
            }
            for (name, value) in vars {
                cmd.env(name, value);
            }
            let output = cmd.output().expect("run tk");
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            runs.push((format!("{args:?} json={json}"), output));
        }
    }
    runs
}

#[test]
fn colour_only_when_forced_and_never_with_no_color_or_json() {
    // Calibration of the reader.
    assert!(has_escape(b"a\x1b[1mb"));
    assert!(!has_escape(b"plain text [1m"));

    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let id = add(&sandbox, &["a task", "-p", "web", "-f", "auth"]);

    for (what, output) in outputs(&sandbox, id, &[]) {
        assert!(!has_escape(&output.stdout), "piped {what}: {output:?}");
    }
    for (what, output) in outputs(&sandbox, id, &[("CLICOLOR_FORCE", "1")]) {
        let json = what.ends_with("json=true");
        assert_eq!(
            has_escape(&output.stdout),
            !json,
            "forced {what}: {output:?}"
        );
    }
    for (what, output) in outputs(&sandbox, id, &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")]) {
        assert!(!has_escape(&output.stdout), "NO_COLOR {what}: {output:?}");
    }

    // Forced colour styles the heading and the status word only; the text is unchanged
    // once the escape sequences are removed.
    let plain = human(&sandbox, &["ls", "-p", "web"]);
    let output = sandbox
        .tk()
        .env("CLICOLOR_FORCE", "1")
        .args(["ls", "-p", "web"])
        .output()
        .expect("run tk");
    let coloured = String::from_utf8(output.stdout).expect("utf-8");
    assert_eq!(strip_sgr(&coloured), plain);
}

/// Removes `ESC [ ... m` sequences.
fn strip_sgr(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('\x1b') {
        out.push_str(&rest[..start]);
        let end = rest[start..].find('m').expect("sequence end");
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

#[test]
fn escape_sequences_in_stored_text_are_printed_as_stored() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let title = "red \x1b[31mtitle\x1b[0m";
    let id = add(&sandbox, &[title, "-p", "web"]);
    let text = human(&sandbox, &["ls", "-p", "web"]);
    assert_eq!(
        text,
        format!("web  (1 open)\n  -\n    #{id}  P2  todo     {title}\n")
    );
    // JSON escapes the control character instead.
    let output = json_run(&sandbox, &["show", &id.to_string()]);
    assert!(!has_escape(&output.stdout));
    assert_eq!(ok_data(&output)["task"]["title"], title);
}

// ---------------------------------------------------------------- review follow-ups

#[test]
fn find_folds_case_per_character_including_final_sigma() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let road = add(&sandbox, &["ΟΔΟΣ", "-p", "web"]);
    let ascii = add(&sandbox, &["Refresh the Token", "-p", "web"]);
    // Calibration: a Greek letter without a context-dependent lowercase matches both ways.
    assert_eq!(find(&sandbox, &["Δ"]), [road]);
    assert_eq!(find(&sandbox, &["δ"]), [road]);
    // The word-final capital sigma matches both lowercase sigmas and itself.
    for query in ["Σ", "σ", "ς", "οδος", "ΟΔΟΣ", "οδοσ"] {
        assert_eq!(find(&sandbox, &[query]), [road], "{query}");
    }
    assert_eq!(find(&sandbox, &["tHE tOKEN"]), [ascii]);
    assert_eq!(find(&sandbox, &["REFRESH"]), [ascii]);
}

#[test]
fn numeric_options_take_values_beginning_with_a_dash() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let id = add(&sandbox, &["task", "-p", "web"]);
    let id = id.to_string();
    for (args, option) in [
        (vec!["edit", &id, "--pri", "-1"], "--pri"),
        (vec!["add", "x", "-p", "web", "--pri", "-1"], "--pri"),
        (vec!["ls", "-p", "web", "--limit", "-1"], "--limit"),
    ] {
        let message = err_message(&json_run(&sandbox, &args), 2, "usage");
        assert!(message.contains("'-1'"), "{args:?}: {message}");
        assert!(message.contains(option), "{args:?}: {message}");
        assert!(
            !message.contains("unexpected argument"),
            "{args:?}: {message}"
        );
    }
    // The task is unchanged and no task was added.
    assert_eq!(show(&sandbox, id.parse().expect("id"))["priority"], 2);
    assert_eq!(ids(&sandbox.ok(&["ls", "-p", "web"])["tasks"]).len(), 1);
}

#[test]
fn invalid_feature_and_tag_filters_are_usage_errors() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    add(&sandbox, &["task", "-p", "web", "-f", "auth", "--tag", "x"]);
    // Calibration: valid but unknown names are accepted as filters.
    assert!(ids(&sandbox.ok(&["ls", "-p", "web", "--tag", "nope"])["tasks"]).is_empty());
    err_message(
        &json_run(&sandbox, &["ls", "-p", "web", "-f", "nope"]),
        3,
        "not_found",
    );
    for args in [
        &["ls", "-p", "web", "--tag", "BAD_TAG"][..],
        &["ls", "--tag", "BAD_TAG"],
        &["ls", "-p", "web", "-f", "BAD_NAME"],
        &["ls", "-f", "-x"],
        &["next", "-p", "web", "-f", "BAD_NAME"],
        &["next", "-f", "Bad"],
    ] {
        err_message(&json_run(&sandbox, args), 2, "usage");
    }
}

#[test]
fn ls_and_find_order_by_age_between_priority_and_id() {
    let sandbox = Sandbox::new();
    project(&sandbox, "web");
    let first = add(&sandbox, &["match first", "-p", "web"]);
    let second = add(&sandbox, &["match second", "-p", "web"]);
    let urgent = add(&sandbox, &["match urgent", "-p", "web", "--pri", "1"]);
    // The lower id is the younger task.
    for (id, at) in [
        (first, "2026-06-01T00:00:00.000Z"),
        (second, "2026-01-01T00:00:00.000Z"),
        (urgent, "2026-12-01T00:00:00.000Z"),
    ] {
        sandbox.sql(
            "UPDATE task SET created_at = ?2 WHERE id = ?1",
            rusqlite::params![id, at],
        );
    }
    // Calibration: the ages were set, so age and id order disagree.
    assert_eq!(
        show(&sandbox, second)["created_at"],
        "2026-01-01T00:00:00.000Z"
    );
    assert!(first < second);

    assert_eq!(
        ids(&sandbox.ok(&["ls", "-p", "web"])["tasks"]),
        [urgent, second, first]
    );
    assert_eq!(
        find(&sandbox, &["match", "-p", "web"]),
        [urgent, second, first]
    );
    // Then id: equal priority and age.
    sandbox.sql(
        "UPDATE task SET created_at = ?1 WHERE id IN (?2, ?3)",
        rusqlite::params!["2026-01-01T00:00:00.000Z", first, second],
    );
    assert_eq!(
        ids(&sandbox.ok(&["ls", "-p", "web"])["tasks"]),
        [urgent, first, second]
    );
    assert_eq!(
        find(&sandbox, &["match", "-p", "web"]),
        [urgent, first, second]
    );
}
