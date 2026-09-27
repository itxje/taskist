//! Integration tests for the entry point: help, version and the error contract.
#![cfg(test)]

pub mod common;

use common::Sandbox;
use predicates::prelude::*;
use serde_json::Value;

fn single_json_line(bytes: &[u8]) -> Value {
    let text = std::str::from_utf8(bytes).expect("utf-8 output");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "expected exactly one line, got {text:?}");
    assert!(
        text.ends_with('\n'),
        "line must be newline-terminated: {text:?}"
    );
    serde_json::from_str(lines[0]).expect("valid JSON line")
}

#[test]
fn version_prints_package_version() {
    Sandbox::new()
        .tk()
        .arg("--version")
        .assert()
        .code(0)
        .stdout(format!("tk {}\n", env!("CARGO_PKG_VERSION")))
        .stderr("");
}

#[test]
fn help_names_the_program_tk() {
    Sandbox::new()
        .tk()
        .arg("--help")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Usage: tk"))
        .stderr("");
}

#[test]
fn help_is_text_even_with_json() {
    Sandbox::new()
        .tk()
        .args(["--json", "--help"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Usage: tk"))
        .stderr("");
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    Sandbox::new()
        .tk()
        .arg("frobnicate")
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::starts_with("error: ").and(predicate::str::contains("frobnicate")));
}

#[test]
fn missing_subcommand_is_a_usage_error() {
    Sandbox::new()
        .tk()
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::starts_with(
            "error: 'tk' requires a subcommand",
        ));
}

fn assert_json_usage_error(output: &std::process::Output) {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "stdout must be empty");
    let envelope = single_json_line(&output.stderr);
    assert_eq!(envelope["ok"], Value::Bool(false));
    assert_eq!(envelope["error"]["code"], "usage");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("frobnicate")),
        "message names the bad argument: {envelope}"
    );
}

#[test]
fn unknown_subcommand_with_json_flag_is_a_json_usage_error() {
    let output = Sandbox::new()
        .tk()
        .args(["--json", "frobnicate"])
        .output()
        .expect("run tk");
    assert_json_usage_error(&output);
}

#[test]
fn unknown_subcommand_with_json_flag_after_it_is_a_json_usage_error() {
    let output = Sandbox::new()
        .tk()
        .args(["frobnicate", "--json"])
        .output()
        .expect("run tk");
    assert_json_usage_error(&output);
}

#[test]
fn unknown_subcommand_with_json_format_variable_is_a_json_usage_error() {
    let output = Sandbox::new()
        .tk()
        .env("TASKIST_FORMAT", "json")
        .arg("frobnicate")
        .output()
        .expect("run tk");
    assert_json_usage_error(&output);
}

#[test]
fn text_format_variable_keeps_human_errors() {
    Sandbox::new()
        .tk()
        .env("TASKIST_FORMAT", "text")
        .arg("frobnicate")
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::starts_with("error: "));
}

#[test]
fn unknown_format_variable_is_a_usage_error() {
    Sandbox::new()
        .tk()
        .env("TASKIST_FORMAT", "xml")
        .arg("frobnicate")
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::starts_with("error: "));
}

/// Runs `tk` through the helper from its working directory, which the shell removes first.
fn tk_in_removed_directory(sandbox: &Sandbox, args: &[&str]) -> std::process::Output {
    sandbox
        .program("/usr/bin/sh")
        .arg("-c")
        .arg(r#"rmdir "$PWD" && exec "$0" "$@""#)
        .arg(assert_cmd::cargo::cargo_bin!("tk"))
        .args(args)
        .output()
        .expect("run tk through sh")
}

#[test]
fn unreadable_current_directory_is_an_internal_error() {
    let sandbox = Sandbox::new();
    let output = tk_in_removed_directory(&sandbox, &["--version"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    assert!(
        stderr.starts_with("error: cannot read the current directory: "),
        "{stderr:?}"
    );
}

#[test]
fn unreadable_current_directory_with_json_is_an_internal_envelope() {
    let sandbox = Sandbox::new();
    let output = tk_in_removed_directory(&sandbox, &["--json", "--version"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let envelope = single_json_line(&output.stderr);
    assert_eq!(envelope["ok"], Value::Bool(false));
    assert_eq!(envelope["error"]["code"], "internal");
}
