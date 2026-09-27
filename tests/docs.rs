//! Checks on the repository documents: the acceptance map of `docs/architecture.md`
//! names tests that exist, the README states the backup rule, and the guide and the
//! README state which empty environment values count as unset.
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

/// The row of criterion `number` in the acceptance map, as written.
fn acceptance_row(architecture: &str, number: u32) -> &str {
    architecture
        .lines()
        .find(|line| line.starts_with(&format!("| {number} |")))
        .expect("a row for the criterion")
}

/// The backticked names in the sentence of `text` that ends with `ending`.
fn names_in_sentence(text: &str, ending: &str) -> Vec<String> {
    let sentence = text
        .split(". ")
        .map(|sentence| sentence.replace('\n', " "))
        .find(|sentence| sentence.trim_end_matches('.').ends_with(ending))
        .expect("a sentence ending with the given words");
    let mut names: Vec<String> = sentence
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    names.sort();
    names
}

fn sorted(names: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
    names.sort();
    names
}

#[test]
fn the_sentence_reader_reads_a_known_text() {
    let text = "Intro `A`. An empty `X` or\n`Y` counts as unset. An empty `Z` counts as set.";
    assert_eq!(
        names_in_sentence(text, "counts as unset"),
        sorted(&["X", "Y"])
    );
    assert_eq!(names_in_sentence(text, "counts as set"), sorted(&["Z"]));
}

#[test]
fn the_guide_and_readme_name_which_empty_values_count_as_unset() {
    // Empty values of the colour variables follow `anstream`: `NO_COLOR` and
    // `CLICOLOR_FORCE` need a non-empty value, `CLICOLOR`, `TERM` and `CI` only need to be set.
    let unset = sorted(&[
        "TASKIST_DB",
        "XDG_DATA_HOME",
        "HOME",
        "TASKIST_PROJECT",
        "TASKIST_ACTOR",
        "USER",
        "NO_COLOR",
        "CLICOLOR_FORCE",
    ]);
    let set = sorted(&["CLICOLOR", "TERM", "CI"]);
    for document in ["docs/guide.md", "README.md"] {
        let text = read(document);
        assert!(
            !text.contains("Empty values count as unset"),
            "{document} claims that every empty value counts as unset"
        );
        assert_eq!(
            names_in_sentence(&text, "counts as unset"),
            unset,
            "{document}"
        );
        assert_eq!(names_in_sentence(&text, "counts as set"), set, "{document}");
    }
}

#[test]
fn criterion_6_names_the_lint_tests_and_the_coverage_measurement() {
    let architecture = read("docs/architecture.md");
    let row = acceptance_row(&architecture, 6);
    assert!(row.contains("`tests/policy.rs::"), "{row}");
    assert!(row.contains("`cargo llvm-cov nextest`"), "{row}");
    assert!(row.contains("whole package"), "{row}");
    assert!(row.contains("`docs/changelog.md`"), "{row}");
    let changelog = read("docs/changelog.md");
    let figure = changelog
        .split("`cargo llvm-cov nextest` over the whole package reports ")
        .nth(1)
        .and_then(|rest| rest.split('%').next())
        .and_then(|figure| figure.parse::<f64>().ok())
        .expect("a coverage figure in docs/changelog.md");
    assert!(figure >= 80.0, "line coverage {figure}% is below 80%");
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
