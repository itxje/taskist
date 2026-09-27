//! Integration tests for `tk guide` and `tk completions`: the guide alone is enough to
//! operate the tool, and it names every command, variable, error code and exit code.
#![cfg(test)]

pub mod common;

use std::ffi::OsString;
use std::path::Path;

use clap::CommandFactory;
use common::{Sandbox, err_message, ok_data};
use serde_json::Value;
use taskist::cli::Cli;
use taskist::error::Error;

/// The guide as `tk guide` prints it.
fn guide_text() -> String {
    let output = Sandbox::new().tk().arg("guide").output().expect("run tk");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    String::from_utf8(output.stdout).expect("utf-8 guide")
}

/// Every line of the fenced `sh` blocks that is neither blank nor a comment, in order.
fn sh_lines(guide: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut in_sh = false;
    let mut in_other = false;
    for line in guide.lines() {
        let fence = line.trim_start();
        if in_sh || in_other {
            if fence.starts_with("```") {
                in_sh = false;
                in_other = false;
            } else if in_sh {
                let command = line.trim();
                if !command.is_empty() && !command.starts_with('#') {
                    lines.push(command.to_owned());
                }
            }
        } else if fence == "```sh" {
            in_sh = true;
        } else if fence.starts_with("```") {
            in_other = true;
        }
    }
    assert!(!in_sh && !in_other, "unterminated fenced block");
    lines
}

/// The words following `tk` on a command line, if it runs `tk`.
fn tk_words(line: &str) -> Option<Vec<&str>> {
    let mut words = line.split_whitespace();
    words.by_ref().find(|word| *word == "tk")?;
    Some(words.collect())
}

/// Every subcommand path of the clap definition, such as `["project", "add"]`.
fn subcommand_paths() -> Vec<Vec<String>> {
    fn walk(command: &clap::Command, prefix: &[String], paths: &mut Vec<Vec<String>>) {
        for sub in command.get_subcommands() {
            if sub.get_name() == "help" {
                continue;
            }
            let mut path = prefix.to_vec();
            path.push(sub.get_name().to_owned());
            if sub.has_subcommands() {
                walk(sub, &path, paths);
            } else {
                paths.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(&Cli::command(), &[], &mut paths);
    paths
}

/// One value of every error variant; the exhaustive match breaks the build when a
/// variant is added without being listed here.
fn every_error() -> Vec<Error> {
    let all = vec![
        Error::Internal(String::new()),
        Error::UnsupportedSchema {
            found: 2,
            supported: 1,
        },
        Error::Usage(String::new()),
        Error::NotFound(String::new()),
        Error::Conflict(String::new()),
        Error::InvalidTransition(String::new()),
    ];
    for error in &all {
        match error {
            Error::Internal(_)
            | Error::UnsupportedSchema { .. }
            | Error::Usage(_)
            | Error::NotFound(_)
            | Error::Conflict(_)
            | Error::InvalidTransition(_) => {}
        }
    }
    all
}

/// Every `TASKIST_*` name that occurs in the library sources.
fn taskist_variables_in_sources() -> Vec<String> {
    fn scan(dir: &Path, names: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("read directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                scan(&path, names);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).expect("read source");
                for (start, _) in text.match_indices("TASKIST_") {
                    let name: String = text[start..]
                        .chars()
                        .take_while(|c| c.is_ascii_uppercase() || *c == '_')
                        .collect();
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
    }
    let mut names = Vec::new();
    scan(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut names,
    );
    names.sort();
    names
}

#[test]
fn the_sh_line_reader_finds_only_sh_block_commands() {
    let text = "intro tk ls\n```sh\n# comment\ntk add \"a b\"\n\nTASKIST_PROJECT=web tk ls\n```\n```json\n{\"tk\":1}\n```\n```text\ntk show 1\n```\n";
    assert_eq!(
        sh_lines(text),
        ["tk add \"a b\"", "TASKIST_PROJECT=web tk ls"]
    );
    assert_eq!(
        tk_words("TASKIST_PROJECT=web tk ls -p x"),
        Some(vec!["ls", "-p", "x"])
    );
    assert_eq!(tk_words("printf x > f"), None);
}

#[test]
fn the_guide_alone_operates_the_tool() {
    let sandbox = Sandbox::new();
    let tk = assert_cmd::cargo::cargo_bin!("tk");
    let tk_dir = tk.parent().expect("binary directory");
    let mut search = vec![tk_dir.to_path_buf()];
    search.extend(["/usr/bin", "/bin"].map(Into::into));
    let path: OsString = std::env::join_paths(search).expect("PATH value");

    // The shell the guide lines run in finds exactly the binary under test as `tk`.
    let found = sandbox
        .program("sh")
        .env("PATH", &path)
        .args(["-c", "command -v tk"])
        .output()
        .expect("run sh");
    assert_eq!(
        String::from_utf8_lossy(&found.stdout).trim_end(),
        tk.to_str().expect("utf-8 path"),
        "{found:?}"
    );

    let lines = sh_lines(&guide_text());
    assert!(lines.len() > 20, "{lines:#?}");
    for line in &lines {
        // Trust boundary: a guide line may not choose the database or leave the sandbox.
        for forbidden in ["TASKIST_DB", "HOME", "XDG_DATA_HOME", "cd ", "sudo"] {
            assert!(!line.contains(forbidden), "{line:?} mentions {forbidden}");
        }
        assert!(!line.ends_with('\\'), "one command per line: {line:?}");
        let output = sandbox
            .program("sh")
            .env("PATH", &path)
            .args(["-c", line])
            .output()
            .expect("run sh");
        assert_eq!(output.status.code(), Some(0), "{line}: {output:?}");
    }

    // The sequence added projects and tasks, listed, edited and closed them.
    let projects = sandbox.ok(&["project", "ls", "--all"]);
    let names: Vec<&str> = projects["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .map(|project| project["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["api", "web"]);

    let list = sandbox.ok(&["ls", "--all", "--all-projects"]);
    let tasks = list["tasks"].as_array().expect("tasks");
    let task = |id: i64| -> &Value {
        tasks
            .iter()
            .find(|task| task["id"] == id)
            .expect("the task is listed")
    };
    assert_eq!(tasks.len(), 6, "{tasks:#?}");
    assert_eq!(task(1)["title"], "Fix the login redirect loop");
    assert_eq!(task(1)["status"], "done");
    assert_eq!(task(1)["tags"], serde_json::json!(["bug", "urgent"]));
    assert_eq!(task(2)["status"], "doing");
    assert_eq!(task(2)["feature"], "documentation");
    assert_eq!(task(3)["project"], "api");
    assert_eq!(task(3)["priority"], 0);
    assert_eq!(task(4)["status"], "todo");
    assert_eq!(task(5)["feature"], "perf");
    assert_eq!(task(6)["project"], "api");
}

#[test]
fn the_guide_names_every_subcommand_variable_error_code_and_exit_code() {
    let guide = guide_text();
    let lines = sh_lines(&guide);
    let commands: Vec<Vec<&str>> = lines.iter().filter_map(|line| tk_words(line)).collect();
    let paths = subcommand_paths();
    assert!(paths.len() > 20, "{paths:?}");
    for path in &paths {
        let example = commands.iter().any(|words| {
            words.len() >= path.len() && words.iter().zip(path).all(|(word, name)| word == name)
        });
        assert!(example, "no sh example runs `tk {}`", path.join(" "));
        assert!(
            guide.contains(&format!("`tk {}", path.join(" "))),
            "the guide does not describe `tk {}`",
            path.join(" ")
        );
    }

    let mut variables = taskist_variables_in_sources();
    assert!(
        variables.contains(&"TASKIST_DB".to_owned()),
        "{variables:?}"
    );
    variables.extend(
        [
            "XDG_DATA_HOME",
            "HOME",
            "USER",
            "NO_COLOR",
            "CLICOLOR",
            "CLICOLOR_FORCE",
            "TERM",
            "CI",
        ]
        .map(str::to_owned),
    );
    for variable in &variables {
        assert!(
            guide.contains(&format!("`{variable}`")),
            "the guide does not name `{variable}`"
        );
    }

    for error in every_error() {
        let row = format!("| `{}` | {} |", error.code(), error.exit_code());
        assert!(guide.contains(&row), "the guide lacks the row {row:?}");
    }
    // The exit code table has one row per exit code, 0 for success and each code an
    // error maps to.
    let mut exits: Vec<u8> = every_error().iter().map(Error::exit_code).collect();
    exits.push(0);
    exits.sort_unstable();
    exits.dedup();
    assert_eq!(exits, [0, 1, 2, 3, 4]);
    for exit in exits {
        let row = format!("| {exit} | ");
        assert!(
            guide.lines().any(|line| line.starts_with(&row)),
            "the guide lacks the exit code row {row:?}"
        );
    }
}

#[test]
fn guide_json_returns_the_guide_in_data_text() {
    let data = Sandbox::new().ok(&["guide"]);
    assert_eq!(data["text"].as_str(), Some(guide_text().as_str()));
    assert!(guide_text().starts_with("# tk guide\n"));
}

#[test]
fn completions_bash_prints_a_script_for_tk() {
    let output = Sandbox::new()
        .tk()
        .args(["completions", "bash"])
        .output()
        .expect("run tk");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let script = String::from_utf8(output.stdout).expect("utf-8 script");
    assert!(script.contains("complete"), "{script}");
    assert!(script.contains("tk"), "{script}");

    let data = Sandbox::new().ok(&["completions", "zsh"]);
    assert!(data["text"].as_str().expect("text").contains("#compdef tk"));
}

#[test]
fn completions_for_an_unknown_shell_is_a_usage_error() {
    let sandbox = Sandbox::new();
    sandbox
        .tk()
        .args(["completions", "tcsh"])
        .assert()
        .code(2)
        .stdout("");
    let output = sandbox
        .tk()
        .args(["--json", "completions", "tcsh"])
        .output()
        .expect("run tk");
    let message = err_message(&output, 2, "usage");
    assert!(message.contains("tcsh"), "{message}");
}

#[test]
fn guide_output_is_the_success_envelope_in_json() {
    let output = Sandbox::new()
        .tk()
        .env("TASKIST_FORMAT", "json")
        .arg("guide")
        .output()
        .expect("run tk");
    let data = ok_data(&output);
    assert!(
        data["text"]
            .as_str()
            .expect("text")
            .contains("## Exit codes")
    );
}
