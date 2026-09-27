//! `tk done <id> <note>`: a note such as `+1` is text, not the id of another task, so it
//! must not close a second task.
#![cfg(test)]

pub mod common;

use common::{Sandbox, ok_data};

fn sandbox() -> Sandbox {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);
    sandbox.ok(&["add", "one", "-p", "web"]);
    sandbox.ok(&["add", "two", "-p", "web"]);
    sandbox
}

fn done(sandbox: &Sandbox, args: &[&str]) -> serde_json::Value {
    ok_data(
        &sandbox
            .tk()
            .args(["--json", "done"])
            .args(args)
            .output()
            .expect("run tk"),
    )
}

#[test]
fn control_a_word_note_leaves_the_other_task_open() {
    let sandbox = sandbox();
    let data = done(&sandbox, &["2", "lgtm"]);
    assert_eq!(data["tasks"].as_array().expect("tasks").len(), 1);
    assert_eq!(sandbox.ok(&["show", "1"])["task"]["status"], "todo");
    assert_eq!(sandbox.ok(&["show", "2"])["notes"][0]["text"], "lgtm");
}

#[test]
fn a_plus_one_note_does_not_close_task_one() {
    let sandbox = sandbox();
    done(&sandbox, &["2", "+1"]);
    assert_eq!(
        sandbox.ok(&["show", "1"])["task"]["status"],
        "todo",
        "task 1 was closed by the note \"+1\""
    );
    assert_eq!(sandbox.ok(&["show", "2"])["notes"][0]["text"], "+1");
}
