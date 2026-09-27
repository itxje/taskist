//! An invalid project name given to `tk edit -p` is a `usage` error before any lookup, so it
//! stays a `usage` error when the task id is unknown too, as an invalid feature name does.
#![cfg(test)]

pub mod common;

use common::{Sandbox, err_message};

fn sandbox() -> Sandbox {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);
    sandbox.ok(&["add", "task", "-p", "web"]);
    sandbox
}

fn edit(sandbox: &Sandbox, args: &[&str]) -> std::process::Output {
    sandbox
        .tk()
        .args(["--json", "edit"])
        .args(args)
        .output()
        .expect("run tk")
}

#[test]
fn controls_known_task_and_invalid_feature_name() {
    let sandbox = sandbox();
    // A known task with an invalid project name is refused as usage.
    err_message(&edit(&sandbox, &["1", "-p", "BAD_NAME"]), 2, "usage");
    // An unknown task with an invalid feature name is refused as usage before the lookup.
    err_message(&edit(&sandbox, &["999", "-f", "BAD_NAME"]), 2, "usage");
    // An unknown task with a valid name is not found.
    err_message(&edit(&sandbox, &["999", "-p", "web"]), 3, "not_found");
}

#[test]
fn an_invalid_project_name_is_usage_even_for_an_unknown_task() {
    let sandbox = sandbox();
    let message = err_message(&edit(&sandbox, &["999", "-p", "BAD_NAME"]), 2, "usage");
    assert!(message.contains("BAD_NAME"), "{message}");
}
