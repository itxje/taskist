//! Integration tests for the status commands `start`, `block`, `done`, `drop` and `reopen`,
//! and for `note`, in human and JSON output.
#![cfg(test)]

pub mod common;

use std::collections::BTreeSet;

use common::{Sandbox, err_message, ok_data};
use serde_json::Value;
use taskist::model::Status;

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

/// A time far before any test runs, so a write that moves `updated_at` is always visible.
const OLD: &str = "2020-01-01T00:00:00.000Z";

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

fn json_run(sandbox: &Sandbox, args: &[&str]) -> std::process::Output {
    sandbox
        .tk()
        .arg("--json")
        .args(args)
        .output()
        .expect("run tk")
}

/// A sandbox with the project `web`.
fn sandbox() -> Sandbox {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);
    sandbox
}

/// Creates a task in `web` through `tk add` and returns its id.
fn add(sandbox: &Sandbox, title: &str) -> i64 {
    sandbox.ok(&["add", title, "-p", "web"])["task"]["id"]
        .as_i64()
        .expect("id")
}

/// A task in `status` whose `updated_at` is [`OLD`].
fn aged(sandbox: &Sandbox, status: Status) -> i64 {
    let id = sandbox.seed_task("web", None, status);
    sandbox.sql(
        "UPDATE task SET updated_at = ?2 WHERE id = ?1",
        rusqlite::params![id, OLD],
    );
    id
}

/// `tk show --json` data: `{task, notes}`.
fn show(sandbox: &Sandbox, id: i64) -> Value {
    sandbox.ok(&["show", &id.to_string()])
}

/// The arguments of the command for `action` on one task; `block` and `drop` get a reason.
fn command(action: &str, id: i64) -> Vec<String> {
    let mut args = vec![action.to_owned(), id.to_string()];
    if matches!(action, "block" | "drop") {
        args.push(format!("{action} reason"));
    }
    args
}

fn run(sandbox: &Sandbox, args: &[String]) -> std::process::Output {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    json_run(sandbox, &args)
}

const ACTIONS: [&str; 5] = ["start", "block", "done", "drop", "reopen"];

const STATUSES: [(Status, &str); 5] = [
    (Status::Todo, "todo"),
    (Status::Doing, "doing"),
    (Status::Blocked, "blocked"),
    (Status::Done, "done"),
    (Status::Dropped, "dropped"),
];

/// The transition table as the design states it: the target of each action and the
/// statuses that change to it. The target itself is a no-op; every other status is invalid.
fn rule(action: &str) -> (&'static str, &'static [&'static str]) {
    const TABLE: [(&str, &str, &[&str]); 5] = [
        ("start", "doing", &["todo", "blocked"]),
        ("block", "blocked", &["todo", "doing"]),
        ("done", "done", &["todo", "doing", "blocked"]),
        ("drop", "dropped", &["todo", "doing", "blocked"]),
        ("reopen", "todo", &["doing", "blocked", "done", "dropped"]),
    ];
    TABLE
        .into_iter()
        .find(|(name, _, _)| *name == action)
        .map(|(_, to, from)| (to, from))
        .expect("known action")
}

// ---------------------------------------------------------------- the table

#[test]
fn the_calibration_fixture_reads_back_what_it_seeds() {
    let sandbox = sandbox();
    for (status, name) in STATUSES {
        let id = aged(&sandbox, status);
        let task = &show(&sandbox, id)["task"];
        assert_eq!(task["status"], name);
        assert_eq!(task["updated_at"], OLD);
        assert_eq!(task["closed_at"].is_null(), status.is_open(), "{task}");
    }
}

#[test]
fn every_change_of_the_table_moves_the_status_and_its_timestamps() {
    let sandbox = sandbox();
    let mut checked = 0;
    for action in ACTIONS {
        let (to, from) = rule(action);
        for (status, name) in STATUSES.into_iter().filter(|(_, n)| from.contains(n)) {
            let id = aged(&sandbox, status);
            let data = ok_data(&run(&sandbox, &command(action, id)));
            let entries = data["tasks"].as_array().expect("tasks");
            assert_eq!(entries.len(), 1, "{data}");
            assert_eq!(entries[0]["changed"], true, "{action} from {name}");
            let task = &entries[0]["task"];
            assert_eq!(task["id"], id);
            assert_eq!(task["status"], to, "{action} from {name}");
            assert_ne!(task["updated_at"], OLD, "{action} from {name}");
            let closed = matches!(to, "done" | "dropped");
            assert_eq!(
                task["closed_at"].is_string(),
                closed,
                "{action} from {name}"
            );
            if closed {
                assert_eq!(task["closed_at"], task["updated_at"]);
            }
            assert_eq!(show(&sandbox, id)["task"], *task, "{action} from {name}");
            checked += 1;
        }
    }
    assert_eq!(checked, 2 + 2 + 3 + 3 + 4);
}

#[test]
fn a_no_op_succeeds_and_writes_nothing() {
    let sandbox = sandbox();
    let mut checked = 0;
    for action in ACTIONS {
        let (to, _) = rule(action);
        let (status, _) = STATUSES
            .into_iter()
            .find(|(_, name)| *name == to)
            .expect("target status");
        let id = aged(&sandbox, status);
        let before = show(&sandbox, id);
        let db = std::fs::read(sandbox.db()).expect("read database");
        let data = ok_data(&run(&sandbox, &command(action, id)));
        assert_eq!(data["tasks"][0]["changed"], false, "{action}");
        assert_eq!(data["tasks"][0]["task"], before["task"], "{action}");
        assert_eq!(show(&sandbox, id), before, "{action}");
        assert_eq!(
            std::fs::read(sandbox.db()).expect("read database"),
            db,
            "{action}"
        );
        checked += 1;
    }
    assert_eq!(checked, ACTIONS.len());

    // `done` with a note on a done task writes no note either.
    let done = aged(&sandbox, Status::Done);
    let before = show(&sandbox, done);
    let data = ok_data(&json_run(&sandbox, &["done", &done.to_string(), "again"]));
    assert_eq!(data["tasks"][0]["changed"], false);
    assert_eq!(show(&sandbox, done), before);
}

#[test]
fn an_invalid_transition_exits_4_and_changes_nothing() {
    let sandbox = sandbox();
    let mut checked = 0;
    for action in ACTIONS {
        let (to, from) = rule(action);
        for (status, name) in STATUSES
            .into_iter()
            .filter(|(_, n)| *n != to && !from.contains(n))
        {
            let id = aged(&sandbox, status);
            let before = show(&sandbox, id);
            let message = err_message(
                &run(&sandbox, &command(action, id)),
                4,
                "invalid_transition",
            );
            assert!(message.contains(&format!("#{id}")), "{message}");
            assert!(message.contains(name), "{message}");
            assert_eq!(show(&sandbox, id), before, "{action} from {name}");
            checked += 1;
        }
    }
    assert_eq!(checked, 2 + 2 + 1 + 1);
}

#[test]
fn an_unknown_id_is_not_found_for_every_command() {
    let sandbox = sandbox();
    for action in ACTIONS {
        let message = err_message(&run(&sandbox, &command(action, 999)), 3, "not_found");
        assert!(message.contains("999"), "{message}");
    }
    let message = err_message(
        &json_run(&sandbox, &["note", "999", "text"]),
        3,
        "not_found",
    );
    assert!(message.contains("999"), "{message}");
}

// ---------------------------------------------------------------- several ids

#[test]
fn several_ids_change_in_argument_order() {
    let sandbox = sandbox();
    let first = add(&sandbox, "first");
    let second = add(&sandbox, "second");
    let doing = add(&sandbox, "doing");
    ok_data(&json_run(&sandbox, &["start", &doing.to_string()]));
    let data = ok_data(&json_run(
        &sandbox,
        &[
            "start",
            &second.to_string(),
            &doing.to_string(),
            &first.to_string(),
        ],
    ));
    let entries = data["tasks"].as_array().expect("tasks");
    let summary: Vec<(i64, bool)> = entries
        .iter()
        .map(|entry| {
            (
                entry["task"]["id"].as_i64().expect("id"),
                entry["changed"].as_bool().expect("changed"),
            )
        })
        .collect();
    assert_eq!(summary, [(second, true), (doing, false), (first, true)]);
    for entry in entries {
        assert_eq!(entry["task"]["status"], "doing");
    }

    let data = ok_data(&json_run(
        &sandbox,
        &["reopen", &first.to_string(), &second.to_string()],
    ));
    assert_eq!(data["tasks"][0]["task"]["status"], "todo");
    assert_eq!(data["tasks"][1]["task"]["status"], "todo");
}

#[test]
fn several_ids_are_all_or_nothing() {
    let sandbox = sandbox();
    let valid = aged(&sandbox, Status::Todo);
    let dropped = aged(&sandbox, Status::Dropped);
    let before = show(&sandbox, valid);

    err_message(
        &json_run(&sandbox, &["done", &valid.to_string(), "999"]),
        3,
        "not_found",
    );
    assert_eq!(show(&sandbox, valid), before);

    err_message(
        &json_run(
            &sandbox,
            &["start", &valid.to_string(), &dropped.to_string()],
        ),
        4,
        "invalid_transition",
    );
    assert_eq!(show(&sandbox, valid), before);

    err_message(
        &json_run(
            &sandbox,
            &["done", &valid.to_string(), &dropped.to_string(), "why"],
        ),
        4,
        "invalid_transition",
    );
    assert_eq!(show(&sandbox, valid), before);
}

#[test]
fn done_takes_leading_integers_as_ids_and_one_note() {
    let sandbox = sandbox();
    let one = add(&sandbox, "one");
    let two = add(&sandbox, "two");
    let data = ok_data(
        &sandbox
            .tk()
            .args(["--json", "--by", "alice", "done"])
            .args([one.to_string(), two.to_string(), "shipped in 1.2".into()])
            .output()
            .expect("run tk"),
    );
    assert_eq!(data["tasks"].as_array().expect("tasks").len(), 2);
    for id in [one, two] {
        let shown = show(&sandbox, id);
        assert_eq!(shown["task"]["status"], "done");
        let notes = shown["notes"].as_array().expect("notes");
        assert_eq!(notes.len(), 1, "{shown}");
        assert_eq!(notes[0]["kind"], "done");
        assert_eq!(notes[0]["text"], "shipped in 1.2");
        assert_eq!(notes[0]["author"], "alice");
    }

    // A note that parses as an integer is passed with --note.
    let three = add(&sandbox, "three");
    ok_data(&json_run(
        &sandbox,
        &["done", &three.to_string(), "--note", "42"],
    ));
    let notes = &show(&sandbox, three)["notes"];
    assert_eq!(notes.as_array().expect("notes").len(), 1);
    assert_eq!(notes[0]["text"], "42");
    assert_eq!(notes[0]["kind"], "done");

    // Without a note, no note is written.
    let four = add(&sandbox, "four");
    ok_data(&json_run(&sandbox, &["done", &four.to_string()]));
    assert_eq!(show(&sandbox, four)["notes"], Value::Array(vec![]));
}

#[test]
fn done_refuses_ambiguous_arguments_as_usage_errors() {
    let sandbox = sandbox();
    let id = add(&sandbox, "one");
    let id = id.to_string();
    let before = show(&sandbox, id.parse().expect("id"));
    for args in [
        vec!["done", "note only"],
        vec!["done", &id, "note", "extra"],
        vec!["done", &id, "note", "--note", "other"],
        vec!["done", &id, "note", "7"],
    ] {
        err_message(&json_run(&sandbox, &args), 2, "usage");
    }
    err_message(&json_run(&sandbox, &["done"]), 2, "usage");
    assert_eq!(show(&sandbox, id.parse().expect("id")), before);
}

// ---------------------------------------------------------------- reasons and closed_at

#[test]
fn block_and_drop_store_the_reason_with_the_resolved_actor() {
    let sandbox = sandbox();
    let blocked = add(&sandbox, "blocked");
    let dropped = add(&sandbox, "dropped");
    ok_data(
        &sandbox
            .tk()
            .args(["--json", "block", &blocked.to_string(), "waiting on schema"])
            .env("TASKIST_ACTOR", "agent:web")
            .env("USER", "alice")
            .output()
            .expect("run tk"),
    );
    ok_data(
        &sandbox
            .tk()
            .args(["--json", "drop", &dropped.to_string(), "not needed"])
            .env("USER", "alice")
            .output()
            .expect("run tk"),
    );
    for (id, kind, text, author) in [
        (blocked, "blocked", "waiting on schema", "agent:web"),
        (dropped, "dropped", "not needed", "alice"),
    ] {
        let shown = show(&sandbox, id);
        assert_eq!(shown["task"]["status"], kind);
        let notes = shown["notes"].as_array().expect("notes");
        assert_eq!(notes.len(), 1, "{shown}");
        assert_eq!(notes[0]["kind"], kind);
        assert_eq!(notes[0]["text"], text);
        assert_eq!(notes[0]["author"], author);
    }

    // Without any actor source the author is `unknown`.
    ok_data(&json_run(&sandbox, &["reopen", &blocked.to_string()]));
    ok_data(&json_run(
        &sandbox,
        &["block", &blocked.to_string(), "again"],
    ));
    let notes = &show(&sandbox, blocked)["notes"];
    assert_eq!(notes[1]["author"], "unknown");
    assert_eq!(notes[1]["text"], "again");
}

#[test]
fn closed_at_is_set_by_done_and_drop_and_cleared_by_reopen() {
    let sandbox = sandbox();
    let id = add(&sandbox, "task");
    let closed_at = |sandbox: &Sandbox| show(sandbox, id)["task"]["closed_at"].clone();
    assert!(closed_at(&sandbox).is_null());
    ok_data(&json_run(&sandbox, &["done", &id.to_string()]));
    assert!(closed_at(&sandbox).is_string());
    ok_data(&json_run(&sandbox, &["reopen", &id.to_string()]));
    assert!(closed_at(&sandbox).is_null());
    ok_data(&json_run(&sandbox, &["drop", &id.to_string(), "no"]));
    assert!(closed_at(&sandbox).is_string());
    ok_data(&json_run(&sandbox, &["reopen", &id.to_string()]));
    assert!(closed_at(&sandbox).is_null());
    ok_data(&json_run(&sandbox, &["start", &id.to_string()]));
    assert!(closed_at(&sandbox).is_null());
}

// ---------------------------------------------------------------- note

#[test]
fn note_appends_a_note_and_moves_updated_at() {
    let sandbox = sandbox();
    let id = aged(&sandbox, Status::Done);
    let before = show(&sandbox, id)["task"].clone();
    let data = ok_data(
        &sandbox
            .tk()
            .args([
                "--json",
                "note",
                &id.to_string(),
                "follow-up in 1.3",
                "--by",
                "bob",
            ])
            .output()
            .expect("run tk"),
    );
    assert_eq!(keys(&data), set(&["task"]));
    assert_eq!(keys(&data["task"]), set(&TASK_KEYS));
    assert_ne!(data["task"]["updated_at"], OLD);
    assert_eq!(data["task"]["status"], "done");
    assert_eq!(data["task"]["closed_at"], before["closed_at"]);
    let shown = show(&sandbox, id);
    assert_eq!(shown["task"], data["task"]);
    let notes = shown["notes"].as_array().expect("notes");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["kind"], "note");
    assert_eq!(notes[0]["text"], "follow-up in 1.3");
    assert_eq!(notes[0]["author"], "bob");
}

// ---------------------------------------------------------------- output shapes

#[test]
fn json_data_has_exactly_the_documented_fields() {
    let sandbox = sandbox();
    let id = add(&sandbox, "task");
    let id = id.to_string();
    for args in [
        vec!["start", &id],
        vec!["block", &id, "why"],
        vec!["done", &id, "note"],
        vec!["reopen", &id],
        vec!["drop", &id, "why"],
        vec!["drop", &id, "why"],
    ] {
        let data = ok_data(&json_run(&sandbox, &args));
        assert_eq!(keys(&data), set(&["tasks"]), "{args:?}");
        for entry in data["tasks"].as_array().expect("tasks") {
            assert_eq!(keys(entry), set(&["task", "changed"]), "{args:?}");
            assert!(entry["changed"].is_boolean());
            assert_eq!(keys(&entry["task"]), set(&TASK_KEYS), "{args:?}");
        }
    }
    let data = ok_data(&json_run(&sandbox, &["note", &id, "text"]));
    assert_eq!(keys(&data), set(&["task"]));
    assert_eq!(keys(&data["task"]), set(&TASK_KEYS));
}

#[test]
fn human_output_names_each_task_and_whether_it_changed() {
    let sandbox = sandbox();
    let one = add(&sandbox, "Fix token refresh race");
    let two = add(&sandbox, "Add SSO login");
    let human = |args: &[String]| {
        let output = sandbox.tk().args(args).output().expect("run tk");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        String::from_utf8(output.stdout).expect("utf-8")
    };
    ok_data(&json_run(&sandbox, &["start", &two.to_string()]));
    assert_eq!(
        human(&["start".into(), one.to_string(), two.to_string()]),
        format!(
            "#{one} is now doing: Fix token refresh race\n\
             #{two} is already doing: Add SSO login\n"
        )
    );
    assert_eq!(
        human(&["note".into(), one.to_string(), "text".into()]),
        format!("noted #{one} in web: Fix token refresh race\n")
    );

    let output = sandbox
        .tk()
        .args(["drop", &one.to_string(), "why", "extra"])
        .output()
        .expect("run tk");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let output = sandbox
        .tk()
        .args(["start", "999"])
        .output()
        .expect("run tk");
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf-8"),
        "error: no task with id 999\n"
    );
}
