//! Tests for the shared integration-test helper itself.
//!
//! They observe what a child process started through the helper actually receives,
//! using the same code path as `Sandbox::tk` and `Sandbox::tk_without_db`.
#![cfg(test)]

pub mod common;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::Sandbox;
use taskist::env::Env;

const ENV: &str = "/usr/bin/env";
const PWD: &str = "/usr/bin/pwd";
const CAT: &str = "/usr/bin/cat";

/// Upper bound for a probe; a child waiting on an open stdin fails instead of hanging.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The environment a child received, as printed by `env`, in output order.
fn observed_env(mut cmd: assert_cmd::Command) -> Vec<(String, String)> {
    let output = cmd.timeout(PROBE_TIMEOUT).output().expect("run env");
    assert!(output.status.success(), "env failed: {output:?}");
    String::from_utf8(output.stdout)
        .expect("utf-8 environment")
        .lines()
        .map(|line| {
            let (name, value) = line.split_once('=').expect("NAME=value line");
            (name.to_owned(), value.to_owned())
        })
        .collect()
}

/// Every way an observed child environment differs from the confined one:
/// exactly the `expected` variables, each an absolute path inside `root`.
fn confinement_violations(
    observed: &[(String, String)],
    expected: &[(OsString, OsString)],
    root: &Path,
) -> Vec<String> {
    let mut violations = Vec::new();
    let mut observed_sorted: Vec<(OsString, OsString)> = observed
        .iter()
        .map(|(name, value)| (name.into(), value.into()))
        .collect();
    observed_sorted.sort();
    let mut expected_sorted = expected.to_vec();
    expected_sorted.sort();
    if observed_sorted != expected_sorted {
        violations.push(format!(
            "environment is {observed_sorted:?}, expected exactly {expected_sorted:?}"
        ));
    }
    for (name, value) in observed {
        let path = Path::new(value);
        if !path.is_absolute() || !path.starts_with(root) {
            violations.push(format!("{name}={value} lies outside {}", root.display()));
        }
    }
    violations
}

fn env_from(observed: &[(String, String)], current_dir: PathBuf) -> Env {
    Env::new(
        observed
            .iter()
            .map(|(name, value)| (name.into(), value.into())),
        current_dir,
        false,
    )
}

fn files_named_db(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            files_named_db(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "db") {
            found.push(path);
        }
    }
}

#[test]
fn confinement_check_rejects_leaking_environments() {
    let root = Path::new("/sandbox");
    let expected: Vec<(OsString, OsString)> = vec![
        ("HOME".into(), "/sandbox/home".into()),
        ("XDG_DATA_HOME".into(), "/sandbox/data".into()),
    ];
    let confined = [
        ("XDG_DATA_HOME".to_owned(), "/sandbox/data".to_owned()),
        ("HOME".to_owned(), "/sandbox/home".to_owned()),
    ];
    assert!(confinement_violations(&confined, &expected, root).is_empty());

    let outside_home = [("HOME".to_owned(), "/nonexistent-outside".to_owned())];
    assert_eq!(
        confinement_violations(&outside_home, &expected, root).len(),
        2,
        "wrong set and a path outside the sandbox"
    );

    let inherited = [
        ("HOME".to_owned(), "/sandbox/home".to_owned()),
        ("XDG_DATA_HOME".to_owned(), "/sandbox/data".to_owned()),
        ("PATH".to_owned(), "/usr/bin".to_owned()),
    ];
    assert_eq!(confinement_violations(&inherited, &expected, root).len(), 2);
}

#[test]
fn helper_environment_resolves_the_database_inside_the_sandbox() {
    let sandbox = Sandbox::new();
    let observed = observed_env(sandbox.program(ENV));
    let violations = confinement_violations(&observed, &sandbox.vars(), sandbox.root());
    assert!(violations.is_empty(), "{violations:#?}");
    let env = env_from(&observed, sandbox.work());
    assert_eq!(env.database_path().expect("resolvable"), sandbox.db());
}

#[test]
fn helper_runs_in_the_work_directory() {
    let sandbox = Sandbox::new();
    for mut cmd in [sandbox.program(PWD), sandbox.program_without_db(PWD)] {
        let output = cmd
            .arg("-P")
            .timeout(PROBE_TIMEOUT)
            .output()
            .expect("run pwd");
        assert!(output.status.success(), "pwd failed: {output:?}");
        let cwd = String::from_utf8(output.stdout).expect("utf-8 path");
        assert_eq!(
            Path::new(cwd.trim_end()),
            sandbox
                .work()
                .canonicalize()
                .expect("canonical work directory")
        );
    }
}

#[test]
fn helper_closes_stdin() {
    let sandbox = Sandbox::new();
    for mut cmd in [sandbox.program(CAT), sandbox.program_without_db(CAT)] {
        // `cat` exits only at end-of-file; an open stdin makes the timeout fail the run.
        let output = cmd.timeout(PROBE_TIMEOUT).output().expect("run cat");
        assert!(
            output.status.success(),
            "cat did not see end-of-file: {output:?}"
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn helper_passes_supplied_input() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .program(CAT)
        .write_stdin("line\n")
        .timeout(PROBE_TIMEOUT)
        .output()
        .expect("run cat");
    assert_eq!(output.stdout, b"line\n");
}

#[test]
fn helper_without_taskist_db_still_stays_inside_the_sandbox() {
    let sandbox = Sandbox::new();
    let expected: Vec<(OsString, OsString)> = sandbox
        .vars()
        .into_iter()
        .filter(|(name, _)| name != "TASKIST_DB")
        .collect();
    let observed = observed_env(sandbox.program_without_db(ENV));
    let violations = confinement_violations(&observed, &expected, sandbox.root());
    assert!(violations.is_empty(), "{violations:#?}");

    // The location tk resolves from exactly what the child received.
    let path = env_from(&observed, sandbox.work())
        .database_path()
        .expect("resolvable");
    assert_eq!(path, sandbox.data_home().join("taskist").join("taskist.db"));

    sandbox.tk_without_db().arg("--version").assert().code(0);
    sandbox.tk_without_db().arg("frobnicate").assert().code(2);

    // Every location tk can derive from its environment lies inside the sandbox root,
    // so scanning the root covers every database it could have created.
    let mut found = Vec::new();
    files_named_db(sandbox.root(), &mut found);
    assert!(
        found.iter().all(|db| db == &path),
        "only the resolved database may appear: {found:?}"
    );
}
