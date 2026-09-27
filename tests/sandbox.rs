//! Tests for the shared integration-test helper itself.
//!
//! They observe what a child process started through the helper actually receives,
//! using the same code path as `Sandbox::tk` and `Sandbox::tk_without_db`, and where
//! `tk` itself, started through those two, creates its database and which directory it runs in.
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

/// Runs `tk project ls`, which opens the database, and returns every database file in the sandbox.
fn databases_after_project_ls(sandbox: &Sandbox, mut cmd: assert_cmd::Command) -> Vec<PathBuf> {
    let mut before = Vec::new();
    files_named_db(sandbox.root(), &mut before);
    assert!(before.is_empty(), "{before:?}");
    // Text output shows that no `TASKIST_FORMAT` from the caller reached `tk`.
    cmd.args(["project", "ls"])
        .timeout(PROBE_TIMEOUT)
        .assert()
        .code(0)
        .stdout("no projects\n");
    let mut found = Vec::new();
    files_named_db(sandbox.root(), &mut found);
    found
}

#[test]
fn tk_creates_the_database_at_taskist_db() {
    let sandbox = Sandbox::new();
    let found = databases_after_project_ls(&sandbox, sandbox.tk());
    assert_eq!(found, [sandbox.db()]);
}

#[test]
fn tk_without_taskist_db_creates_the_database_under_xdg_data_home() {
    let sandbox = Sandbox::new();
    let found = databases_after_project_ls(&sandbox, sandbox.tk_without_db());
    assert_eq!(
        found,
        [sandbox.data_home().join("taskist").join("taskist.db")]
    );
}

#[test]
fn tk_without_taskist_db_or_xdg_data_home_creates_the_database_under_home() {
    let sandbox = Sandbox::new();
    let mut cmd = sandbox.tk_without_db();
    cmd.env_remove("XDG_DATA_HOME");
    let found = databases_after_project_ls(&sandbox, cmd);
    assert_eq!(
        found,
        [sandbox
            .home()
            .join(".local")
            .join("share")
            .join("taskist")
            .join("taskist.db")]
    );
}

/// A child process that is stopped when the test ends, also when it fails.
#[cfg(target_os = "linux")]
struct Holder(std::process::Child);

#[cfg(target_os = "linux")]
impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `tk` started through the helper after its working directory was removed fails
/// with the internal current-directory error, so it runs in that directory.
///
/// A holder process keeps the removed directory as its own working directory, and
/// `work()` becomes a link to `/proc/<holder>/cwd`, through which the helper can still
/// enter the removed directory.
#[cfg(target_os = "linux")]
#[test]
fn tk_runs_in_the_work_directory() {
    let sandbox = Sandbox::new();
    for (name, make) in [
        ("tk", Sandbox::tk as fn(&Sandbox) -> assert_cmd::Command),
        ("tk_without_db", Sandbox::tk_without_db),
    ] {
        make(&sandbox)
            .arg("--version")
            .timeout(PROBE_TIMEOUT)
            .assert()
            .code(0);

        let holder = Holder(
            std::process::Command::new("/usr/bin/sleep")
                .arg("60")
                .current_dir(sandbox.work())
                .stdin(std::process::Stdio::null())
                .spawn()
                .expect("start holder"),
        );
        std::fs::remove_dir_all(sandbox.work()).expect("remove work directory");
        std::os::unix::fs::symlink(format!("/proc/{}/cwd", holder.0.id()), sandbox.work())
            .expect("link work directory");
        let output = make(&sandbox)
            .arg("--version")
            .timeout(PROBE_TIMEOUT)
            .output()
            .expect("run tk");
        drop(holder);
        std::fs::remove_file(sandbox.work()).expect("remove link");
        std::fs::create_dir(sandbox.work()).expect("recreate work directory");

        assert_eq!(output.status.code(), Some(1), "{name}: {output:?}");
        assert!(output.stdout.is_empty(), "{name}: {output:?}");
        let stderr = String::from_utf8(output.stderr).expect("utf-8");
        assert!(
            stderr.starts_with("error: cannot read the current directory: "),
            "{name}: {stderr:?}"
        );
    }
}

/// The tests that start `tk` through the helper and check where it creates its database.
const CONFINEMENT_TESTS: [&str; 3] = [
    "tk_creates_the_database_at_taskist_db",
    "tk_without_taskist_db_creates_the_database_under_xdg_data_home",
    "tk_without_taskist_db_or_xdg_data_home_creates_the_database_under_home",
];

/// Every file below `dir`.
fn files_below(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            files_below(&path, found);
        } else {
            found.push(path);
        }
    }
}

fn decoy_dir() -> tempfile::TempDir {
    let decoy = tempfile::tempdir().expect("create decoy directory");
    for dir in ["home", "data"] {
        std::fs::create_dir(decoy.path().join(dir)).expect("create decoy subdirectory");
    }
    decoy
}

/// Sets every variable `tk` reads to a decoy inside `decoy`; stdin is closed.
fn with_decoys(mut cmd: std::process::Command, decoy: &Path) -> std::process::Command {
    cmd.env("TASKIST_DB", decoy.join("user.db"))
        .env("TASKIST_FORMAT", "json")
        .env("HOME", decoy.join("home"))
        .env("XDG_DATA_HOME", decoy.join("data"))
        .stdin(std::process::Stdio::null());
    cmd
}

/// Runs the confinement tests again in a child of this test binary whose environment
/// carries decoy values for every variable `tk` reads, all pointing into a second
/// temporary directory. A helper that lets the caller's environment through hands the
/// decoys to `tk`: the database lands in the decoy directory or the output turns into JSON.
#[test]
fn confinement_tests_hold_when_the_caller_sets_taskist_variables() {
    // Control: an unconfined `tk` given the decoy environment writes into the decoy
    // directory and prints JSON, so the decoys are strong enough to be noticed.
    let control = decoy_dir();
    let output = with_decoys(
        std::process::Command::new(assert_cmd::cargo::cargo_bin!("tk")),
        control.path(),
    )
    .args(["project", "ls"])
    .output()
    .expect("run tk");
    assert_eq!(output.stdout, b"{\"ok\":true,\"data\":[]}\n", "{output:?}");
    assert!(control.path().join("user.db").is_file());

    let decoy = decoy_dir();
    let output = with_decoys(
        std::process::Command::new(std::env::current_exe().expect("test binary")),
        decoy.path(),
    )
    .args(CONFINEMENT_TESTS)
    .args(["--exact", "--test-threads=1"])
    .output()
    .expect("run the test binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{output:?}");
    assert!(
        stdout.contains(&format!(
            "test result: ok. {} passed",
            CONFINEMENT_TESTS.len()
        )),
        "every confinement test ran: {stdout}"
    );
    let mut touched = Vec::new();
    files_below(decoy.path(), &mut touched);
    assert!(
        touched.is_empty(),
        "the decoy directory was written: {touched:?}"
    );
}
