//! Tests that drive the task commands through the library, in this process, with a
//! hand-built environment pointing into a sandbox.
#![cfg(test)]

pub mod common;

use std::ffi::OsString;

use common::Sandbox;
use taskist::command::task::{self, ListFilter, NewTaskInput, TaskEdit};
use taskist::env::Env;
use taskist::error::Error;
use taskist::model::{NoteKind, Status};
use taskist::scope::{Request, Source};

fn env(sandbox: &Sandbox, extra: &[(&str, &str)]) -> Env {
    let mut vars = sandbox.vars();
    vars.extend(
        extra
            .iter()
            .map(|(name, value)| (OsString::from(name), OsString::from(value))),
    );
    Env::new(vars, sandbox.work(), false)
}

const fn project(name: &str) -> Request<'_> {
    Request {
        project: Some(name),
        all_projects: false,
    }
}

fn input(title: &str) -> NewTaskInput<'_> {
    NewTaskInput {
        title,
        feature: None,
        priority: None,
        tags: &[],
        body: None,
    }
}

#[test]
fn the_task_commands_work_through_the_library() {
    let sandbox = Sandbox::new();
    let env = env(&sandbox, &[("TASKIST_ACTOR", "lib")]);
    taskist::command::project::add(&env, "web", None, None).unwrap();
    taskist::command::project::add(&env, "api", None, None).unwrap();

    let tags = ["ui".to_owned()];
    let first = task::add(
        &env,
        project("web"),
        NewTaskInput {
            feature: Some("auth"),
            priority: Some(1),
            tags: &tags,
            body: Some("text"),
            ..input("First")
        },
        None,
    )
    .unwrap()
    .task;
    assert_eq!(
        (
            first.feature.as_deref(),
            first.priority,
            first.created_by.as_str()
        ),
        (Some("auth"), 1, "lib")
    );
    let second = task::add(&env, project("web"), input("Second"), Some("flag"))
        .unwrap()
        .task;
    assert_eq!(second.created_by, "flag");
    let other = task::add(&env, project("api"), input("Other"), None)
        .unwrap()
        .task;

    let list = task::list(&env, Request::default(), ListFilter::default()).unwrap();
    assert_eq!(list.scope.source, Source::None);
    let ids: Vec<i64> = list.tasks.iter().map(|task| task.id).collect();
    assert_eq!(ids, [other.id, first.id, second.id]);
    assert_eq!(list.open.get("web"), Some(&2));

    let tagged = task::list(
        &env,
        project("web"),
        ListFilter {
            tag: Some("ui"),
            ..ListFilter::default()
        },
    )
    .unwrap();
    assert_eq!(tagged.tasks.len(), 1);

    sandbox.set_status(second.id, Status::Blocked);
    sandbox.note(second.id, NoteKind::Blocked, "waiting");
    let blocked = task::list(
        &env,
        project("web"),
        ListFilter {
            statuses: &[Status::Blocked],
            ..ListFilter::default()
        },
    )
    .unwrap();
    assert_eq!(
        blocked.reasons.get(&second.id).map(String::as_str),
        Some("waiting")
    );

    let shown = task::show(&env, second.id).unwrap();
    assert_eq!(shown.notes.len(), 1);
    assert_eq!(shown.notes[0].author, "seed");

    let moved = task::edit(
        &env,
        first.id,
        TaskEdit {
            project: Some("api"),
            tags: &["-ui".to_owned(), "+x".to_owned()],
            ..TaskEdit::default()
        },
    )
    .unwrap()
    .task;
    assert_eq!(
        (
            moved.project.as_str(),
            moved.feature.as_deref(),
            moved.tags.as_slice()
        ),
        ("api", Some("auth"), &["x".to_owned()][..])
    );

    let next = task::next(&env, project("api"), None).unwrap();
    assert_eq!(next.task.map(|task| task.id), Some(first.id));
    let next = task::next(&env, project("api"), Some("auth")).unwrap();
    assert_eq!(next.task.map(|task| task.id), Some(first.id));

    let found = task::find(&env, Request::default(), "WAIT", false).unwrap();
    assert_eq!(found.tasks.len(), 1);
    assert_eq!(found.tasks[0].id, second.id);

    let err = task::edit(&env, first.id, TaskEdit::default()).unwrap_err();
    assert!(matches!(err, Error::Usage(_)), "{err:?}");
    let err = task::show(&env, 999).unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
    let err = task::add(&env, Request::default(), input("x"), None).unwrap_err();
    assert!(matches!(err, Error::Usage(_)), "{err:?}");
}

#[test]
fn run_dispatches_every_task_command() {
    let sandbox = Sandbox::new();
    let env = env(&sandbox, &[]);
    let run = |args: &[&str]| {
        let args: Vec<OsString> = std::iter::once("tk")
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect();
        taskist::run(&args, &env)
    };
    assert_eq!(run(&["--json", "project", "add", "web"]), 0);
    assert_eq!(run(&["--json", "add", "one", "-p", "web", "-f", "auth"]), 0);
    assert_eq!(run(&["add", "two", "-p", "web", "--tag", "x"]), 0);
    for args in [
        &["ls", "-p", "web"][..],
        &["--json", "ls", "--status", "todo,doing", "--limit", "1"][..],
        &["show", "1"][..],
        &["--json", "show", "1"][..],
        &["edit", "1", "--title", "renamed", "--tag", "-x"][..],
        &["--json", "edit", "2", "--no-feature"][..],
        &["next"][..],
        &["--json", "next", "--all-projects"][..],
        &["find", "renamed"][..],
        &["--json", "find", "two", "--all"][..],
    ] {
        assert_eq!(run(args), 0, "{args:?}");
    }
    assert_eq!(run(&["--json", "show", "99"]), 3);
    assert_eq!(run(&["edit", "1"]), 2);
    assert_eq!(run(&["--json", "ls", "-f", "nope"]), 3);
}
