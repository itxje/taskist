//! Checks on the repository documents: the acceptance map of `docs/architecture.md`
//! names tests that exist, and the README states the backup rule.
#![cfg(test)]

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative)).expect("read document")
}

/// The rows of the acceptance map: criterion number and the `path::test` names in the row.
fn acceptance_rows(architecture: &str) -> Vec<(u32, Vec<(String, String)>)> {
    let section = architecture
        .split("\n## ")
        .find(|section| section.starts_with("MVP acceptance map"))
        .expect("an `## MVP acceptance map` section");
    section
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let criterion = cells.get(1)?.parse::<u32>().ok()?;
            let tests = line
                .split('`')
                .skip(1)
                .step_by(2)
                .filter_map(|name| {
                    let (path, test) = name.rsplit_once("::")?;
                    Some((path.to_owned(), test.to_owned()))
                })
                .collect();
            Some((criterion, tests))
        })
        .collect()
}

/// Whether `file` defines a function called `name`.
fn defines(file: &Path, name: &str) -> bool {
    std::fs::read_to_string(file).is_ok_and(|text| text.contains(&format!("fn {name}(")))
}

#[test]
fn the_map_reader_reads_a_known_table() {
    let text = "# A\n\n## MVP acceptance map\n\n| # | Criterion | Tests |\n|---|---|---|\n\
                | 1 | one | `tests/a.rs::x`, `src/b.rs::y` |\n| 2 | two | `tests/c.rs::z` |\n\n## Next\n| 3 | x | `tests/d.rs::w` |\n";
    assert_eq!(
        acceptance_rows(text),
        [
            (
                1,
                vec![
                    ("tests/a.rs".to_owned(), "x".to_owned()),
                    ("src/b.rs".to_owned(), "y".to_owned())
                ]
            ),
            (2, vec![("tests/c.rs".to_owned(), "z".to_owned())]),
        ]
    );
    assert!(defines(&root().join("tests/docs.rs"), "defines"));
    assert!(!defines(&root().join("tests/docs.rs"), "no_such_function"));
}

#[test]
fn every_mvp_criterion_maps_to_tests_that_exist() {
    let rows = acceptance_rows(&read("docs/architecture.md"));
    let criteria: Vec<u32> = rows.iter().map(|(criterion, _)| *criterion).collect();
    assert_eq!(criteria, [1, 2, 3, 4, 5, 6]);
    for (criterion, tests) in &rows {
        assert!(!tests.is_empty(), "criterion {criterion} names no test");
        for (path, test) in tests {
            assert!(
                path.starts_with("tests/") || path.starts_with("src/"),
                "criterion {criterion}: {path} is outside tests/ and src/"
            );
            assert!(
                defines(&root().join(path), test),
                "criterion {criterion}: {path} defines no test {test}"
            );
        }
    }
}

#[test]
fn the_readme_says_backups_are_copies_of_the_database_file() {
    let readme = read("README.md");
    assert!(readme.contains("copy of the database file"), "{readme}");
    assert!(readme.contains("cannot be imported"), "{readme}");
}
