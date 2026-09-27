//! `find` must match two texts that differ only in letter case: the capital sharp s
//! `ẞ` (U+1E9E) has the lowercase `ß`.
#![cfg(test)]

pub mod common;

use common::{Sandbox, ok_data};

fn titles(sandbox: &Sandbox, query: &str) -> Vec<String> {
    let output = sandbox
        .tk()
        .args(["--json", "find", query, "-p", "web"])
        .output()
        .expect("run tk");
    ok_data(&output)["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|task| task["title"].as_str().expect("title").to_owned())
        .collect()
}

#[test]
fn find_matches_capital_and_small_sharp_s() {
    // Independent check of the case pair itself, by the standard library.
    assert_eq!('\u{1e9e}'.to_lowercase().to_string(), "ß");
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);
    sandbox.ok(&["add", "GROẞ", "-p", "web"]);
    sandbox.ok(&["add", "straße", "-p", "web"]);
    // Calibration: each title is found by its own spelling and by an ASCII case variant.
    assert_eq!(titles(&sandbox, "GROẞ"), ["GROẞ"]);
    assert_eq!(titles(&sandbox, "STRA"), ["straße"]);
    // The lowercase of the title must find it, and the uppercase of the other.
    assert_eq!(titles(&sandbox, "groß"), ["GROẞ"], "groß vs GROẞ");
    assert_eq!(titles(&sandbox, "STRAẞE"), ["straße"], "STRAẞE vs straße");
}
