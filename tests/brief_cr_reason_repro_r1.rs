//! A blocked reason whose lines end in a bare carriage return must stay inside its item.
//!
//! `CommonMark` (0.31, section 2.1) ends a line at a line feed, a carriage return not
//! followed by a line feed, or a carriage return followed by a line feed. The model treats
//! a carriage return as a line terminator too (`LINE_TERMINATORS`).
#![cfg(test)]

pub mod common;

use common::Sandbox;

/// The lines of `text` as a `CommonMark` reader splits them.
fn markdown_lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(str::to_owned)
        .collect()
}

/// Top-level headings and list items of the digest: lines at column 0 that open one.
fn top_level_blocks(text: &str) -> Vec<String> {
    markdown_lines(text)
        .into_iter()
        .filter(|line| line.starts_with('#') || line.starts_with("- "))
        .collect()
}

fn brief_of(reason: &str) -> String {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);
    sandbox.ok(&["add", "Paginate /orders", "-p", "web"]);
    sandbox.ok(&["block", "1", reason]);
    let data = sandbox.ok(&["brief", "-p", "web"]);
    data["text"].as_str().expect("text").to_owned()
}

#[test]
fn a_reason_with_carriage_returns_forms_no_heading_or_item() {
    let expected = [
        "# web (1 open)",
        "## (no feature)",
        "- #1 P2 blocked Paginate /orders",
        "- #1 Paginate /orders: first",
    ];
    // Control: line feeds are already indented, so the reader finds only the digest's own
    // blocks.
    let control = brief_of("first\n# api (9 open)\n- #1 forged");
    assert_eq!(top_level_blocks(&control), expected, "{control:?}");

    let text = brief_of("first\r# api (9 open)\r- #1 forged");
    assert_eq!(top_level_blocks(&text), expected, "{text:?}");
}
