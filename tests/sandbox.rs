//! Tests for the shared integration-test helper itself.
#![cfg(test)]

pub mod common;

use std::path::{Path, PathBuf};

use common::Sandbox;
use taskist::env::Env;

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
fn helper_environment_resolves_the_database_inside_the_sandbox() {
    let sandbox = Sandbox::new();
    let env = Env::new(sandbox.vars(), sandbox.work(), false);
    assert_eq!(env.database_path().expect("resolvable"), sandbox.db());
}

#[test]
fn helper_without_taskist_db_still_stays_inside_the_sandbox() {
    let sandbox = Sandbox::new();
    let vars = sandbox
        .vars()
        .into_iter()
        .filter(|(name, _)| name != "TASKIST_DB");
    let env = Env::new(vars, sandbox.work(), false);
    let path = env.database_path().expect("resolvable");
    assert_eq!(path, sandbox.data_home().join("taskist").join("taskist.db"));
    assert!(path.starts_with(sandbox.root()));

    sandbox.tk_without_db().arg("--version").assert().code(0);
    sandbox.tk_without_db().arg("frobnicate").assert().code(2);

    let mut found = Vec::new();
    files_named_db(sandbox.root(), &mut found);
    assert!(
        found.iter().all(|db| db == &path),
        "only the resolved database may appear: {found:?}"
    );
}
