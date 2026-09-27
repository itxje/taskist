//! Tests that read repository files: crate roots, lint table, licence, decision record
//! and the rule that only `main` reads the process environment.
#![cfg(test)]

use std::path::{Path, PathBuf};

fn package_file(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path)
        .map_err(|err| format!("read {}: {err}", path.display()))
        .unwrap()
}

/// The lines of a TOML table, without comments and blank lines, whitespace-normalized.
fn table_lines(toml: &str, header: &str) -> Vec<String> {
    let mut lines = toml.lines().skip_while(|line| line.trim() != header);
    assert!(lines.next().is_some(), "table {header} missing");
    lines
        .take_while(|line| !line.trim_start().starts_with('['))
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

#[test]
fn table_reader_calibration() {
    let toml = "[a]\nx = 1\n\n# note\n[b]\n  y  =  2 \n[c]\n";
    assert_eq!(table_lines(toml, "[a]"), ["x = 1"]);
    assert_eq!(table_lines(toml, "[b]"), ["y = 2"]);
    assert!(table_lines(toml, "[c]").is_empty());
}

#[test]
fn both_crate_roots_forbid_unsafe_code() {
    for root in ["src/lib.rs", "src/main.rs"] {
        let source = package_file(root);
        assert!(
            source
                .lines()
                .any(|line| line.trim() == "#![forbid(unsafe_code)]"),
            "{root} lacks #![forbid(unsafe_code)]"
        );
    }
}

#[test]
fn rust_lint_table_matches_policy() {
    let manifest = package_file("Cargo.toml");
    assert_eq!(
        table_lines(&manifest, "[lints.rust]"),
        [
            r#"warnings = { level = "deny", priority = -2 }"#,
            r#"unsafe_code = "forbid""#,
            r#"missing_debug_implementations = "warn""#,
            r#"missing_docs = "warn""#,
            r#"unreachable_pub = "warn""#,
            r#"elided_lifetimes_in_paths = "warn""#,
        ]
    );
}

#[test]
fn clippy_lint_table_matches_policy() {
    let manifest = package_file("Cargo.toml");
    assert_eq!(
        table_lines(&manifest, "[lints.clippy]"),
        [
            r#"pedantic = { level = "warn", priority = -1 }"#,
            r#"nursery = { level = "warn", priority = -1 }"#,
            r#"cargo = { level = "warn", priority = -1 }"#,
            r#"unwrap_used = "deny""#,
            r#"expect_used = "deny""#,
            r#"panic = "deny""#,
            r#"dbg_macro = "deny""#,
            r#"print_stdout = "deny""#,
            r#"print_stderr = "deny""#,
            r#"module_name_repetitions = "allow""#,
            r#"must_use_candidate = "allow""#,
            r#"missing_errors_doc = "allow""#,
            r#"missing_panics_doc = "allow""#,
            r#"cargo_common_metadata = "allow""#,
            r#"multiple_crate_versions = "allow""#,
        ]
    );
}

#[test]
fn licence_is_mit_with_the_project_copyright() {
    let licence = package_file("LICENSE");
    assert_eq!(licence.lines().next(), Some("MIT License"));
    assert!(
        licence
            .lines()
            .any(|line| line == "Copyright (c) 2026 taskist contributors")
    );
    assert!(
        licence.contains(
            "Permission is hereby granted, free of charge, to any person obtaining a copy"
        )
    );
    assert!(licence.contains("THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND"));
}

#[test]
fn bundled_sqlite_decision_record_has_its_four_parts() {
    let record = package_file("docs/decisions/2026-09-27-bundled-sqlite.md");
    for heading in [
        "## Context",
        "## Decision",
        "## Consequences",
        "## Sunset condition",
    ] {
        assert!(
            record.lines().any(|line| line == heading),
            "decision record lacks {heading}"
        );
    }
}

fn rust_sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn only_main_reads_the_process_environment() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    rust_sources(&src, &mut sources);
    let main = src.join("main.rs");
    assert!(sources.contains(&main));
    assert!(package_file("src/main.rs").contains("std::env::vars_os"));

    let forbidden = [
        "std::env",
        "env::var",
        "env::current_dir",
        "env::args",
        "env::set_current_dir",
    ];
    for path in sources.iter().filter(|path| **path != main) {
        let source = std::fs::read_to_string(path).expect("read source");
        for pattern in forbidden {
            assert!(
                !source.contains(pattern),
                "{} mentions {pattern}",
                path.display()
            );
        }
    }
}
