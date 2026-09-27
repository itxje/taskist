//! Integration tests for `brief`, `export` and `import`.
#![cfg(test)]

pub mod common;

use std::path::PathBuf;

use common::{Sandbox, err_message, ok_data, real};
use serde_json::{Value, json};

fn project(sandbox: &Sandbox, args: &[&str]) {
    sandbox.ok(&[&["project", "add"][..], args].concat());
}

/// Runs `tk add` with `args` and returns the new task id as text.
fn add(sandbox: &Sandbox, args: &[&str]) -> String {
    let data = sandbox.ok(&[&["add"][..], args].concat());
    data["task"]["id"].as_i64().expect("id").to_string()
}

fn json_run(sandbox: &Sandbox, args: &[&str]) -> std::process::Output {
    sandbox
        .tk()
        .arg("--json")
        .args(args)
        .output()
        .expect("run tk")
}

fn stdout(output: &std::process::Output) -> String {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    String::from_utf8(output.stdout.clone()).expect("utf-8 output")
}

fn task_count(sandbox: &Sandbox) -> i64 {
    rusqlite::Connection::open(sandbox.db())
        .expect("open database")
        .query_row("SELECT count(*) FROM task", [], |row| row.get(0))
        .expect("count tasks")
}

/// Seeds the `ls` layout example: open tasks in two features and without one, a blocked
/// task with a reason, and closed tasks that the brief leaves out.
fn seed_brief(sandbox: &Sandbox) {
    project(sandbox, &["web"]);
    project(sandbox, &["api"]);
    add(
        sandbox,
        &[
            "Fix token refresh race",
            "-p",
            "web",
            "-f",
            "auth",
            "--pri",
            "1",
        ],
    );
    add(sandbox, &["Add SSO login", "-p", "web", "-f", "auth"]);
    add(sandbox, &["Paginate /orders", "-p", "web", "-f", "api"]);
    add(sandbox, &["Clean up README", "-p", "web", "--pri", "3"]);
    add(sandbox, &["Finished work", "-p", "web", "-f", "auth"]);
    add(sandbox, &["Abandoned work", "-p", "web"]);
    add(sandbox, &["Rate limits", "-p", "api", "--pri", "0"]);
    add(sandbox, &["Blocked without reason", "-p", "api"]);
    sandbox.ok(&["start", "1"]);
    sandbox.ok(&["block", "3", "waiting on schema"]);
    sandbox.ok(&["done", "5"]);
    sandbox.ok(&["drop", "6", "not needed"]);
    sandbox.sql("UPDATE task SET status = 'blocked' WHERE id = 8", []);
}

const WEB_BRIEF: &str = "\
# web (4 open)

## api
- #3 P2 blocked Paginate /orders

## auth
- #1 P1 doing Fix token refresh race
- #2 P2 todo Add SSO login

## (no feature)
- #4 P3 todo Clean up README

Blocked:
- #3 Paginate /orders: waiting on schema
";

const API_BRIEF: &str = "\
# api (2 open)

## (no feature)
- #7 P0 todo Rate limits
- #8 P2 blocked Blocked without reason

Blocked:
- #8 Blocked without reason: no reason recorded
";

#[test]
fn brief_lists_open_tasks_by_feature_and_flags_blocked_ones() {
    let sandbox = Sandbox::new();
    seed_brief(&sandbox);
    let text = stdout(
        &sandbox
            .tk()
            .args(["brief", "-p", "web"])
            .output()
            .expect("run tk"),
    );
    assert_eq!(text, WEB_BRIEF);

    let data = ok_data(&json_run(&sandbox, &["brief", "-p", "web"]));
    assert_eq!(data["text"], WEB_BRIEF, "{data}");
    assert_eq!(data["scope"], json!({"project": "web", "source": "flag"}));
}

#[test]
fn a_multi_line_blocked_reason_stays_inside_its_item() {
    let sandbox = Sandbox::new();
    project(&sandbox, &["web"]);
    add(&sandbox, &["Paginate /orders", "-p", "web"]);
    sandbox.ok(&["block", "1", "first\n# api (9 open)\n- #1 forged"]);
    let expected = "\
# web (1 open)

## (no feature)
- #1 P2 blocked Paginate /orders

Blocked:
- #1 Paginate /orders: first
  # api (9 open)
  - #1 forged
";
    let text = stdout(
        &sandbox
            .tk()
            .args(["brief", "-p", "web"])
            .output()
            .expect("run tk"),
    );
    assert_eq!(text, expected);
    let data = ok_data(&json_run(&sandbox, &["brief", "-p", "web"]));
    assert_eq!(data["text"], expected, "{data}");
}

/// Every line break a reader may honour: line feed first as the control, then CRLF, and
/// the single terminators the model keeps out of titles.
const BREAKS: [&str; 8] = [
    "\n", "\r\n", "\r", "\u{0B}", "\u{0C}", "\u{85}", "\u{2028}", "\u{2029}",
];

/// The lines of `text`, split at every line break in [`BREAKS`], which is stricter than a
/// `CommonMark` reader (0.31, section 2.1) that breaks at LF, CR and CRLF only.
fn markdown_lines(text: &str) -> Vec<&str> {
    text.split("\r\n")
        .flat_map(|part| {
            part.split([
                '\n', '\r', '\u{0B}', '\u{0C}', '\u{85}', '\u{2028}', '\u{2029}',
            ])
        })
        .collect()
}

/// Top-level headings and list items: lines at column 0 that open one.
fn top_level_blocks(text: &str) -> Vec<&str> {
    markdown_lines(text)
        .into_iter()
        .filter(|line| line.starts_with('#') || line.starts_with("- "))
        .collect()
}

#[test]
fn a_blocked_reason_breaks_at_every_line_terminator_inside_its_item() {
    let expected = "\
# web (1 open)

## (no feature)
- #1 P2 blocked Paginate /orders

Blocked:
- #1 Paginate /orders: first
  # api (9 open)
  - #1 forged
";
    assert_eq!(
        top_level_blocks(expected),
        [
            "# web (1 open)",
            "## (no feature)",
            "- #1 P2 blocked Paginate /orders",
            "- #1 Paginate /orders: first",
        ]
    );
    for line_break in BREAKS {
        let sandbox = Sandbox::new();
        project(&sandbox, &["web"]);
        add(&sandbox, &["Paginate /orders", "-p", "web"]);
        let reason = ["first", "# api (9 open)", "- #1 forged"].join(line_break);
        sandbox.ok(&["block", "1", &reason]);
        let text = stdout(
            &sandbox
                .tk()
                .args(["brief", "-p", "web"])
                .output()
                .expect("run tk"),
        );
        assert_eq!(text, expected, "{line_break:?}");
        let data = ok_data(&json_run(&sandbox, &["brief", "-p", "web"]));
        assert_eq!(data["text"], expected, "{line_break:?}");
    }
}

#[test]
fn brief_covers_every_project_without_scope_and_with_all_projects() {
    let sandbox = Sandbox::new();
    seed_brief(&sandbox);
    let expected = format!("{API_BRIEF}\n{WEB_BRIEF}");
    let text = stdout(&sandbox.tk().arg("brief").output().expect("run tk"));
    assert_eq!(text, expected);

    let data = ok_data(
        &sandbox
            .tk()
            .args(["--json", "brief", "--all-projects"])
            .env("TASKIST_PROJECT", "web")
            .output()
            .expect("run tk"),
    );
    assert_eq!(data["text"], expected.as_str(), "{data}");
    assert_eq!(data["scope"], json!({"project": null, "source": "none"}));

    let from_env = stdout(
        &sandbox
            .tk()
            .arg("brief")
            .env("TASKIST_PROJECT", "api")
            .output()
            .expect("run tk"),
    );
    assert_eq!(from_env, API_BRIEF);
}

#[test]
fn brief_of_a_scope_without_open_work() {
    let sandbox = Sandbox::new();
    let empty = stdout(&sandbox.tk().arg("brief").output().expect("run tk"));
    assert_eq!(empty, "no open tasks\n");
    project(&sandbox, &["web"]);
    let text = stdout(
        &sandbox
            .tk()
            .args(["brief", "-p", "web"])
            .output()
            .expect("run tk"),
    );
    assert_eq!(text, "# web (0 open)\n");
    let message = err_message(
        &json_run(&sandbox, &["brief", "-p", "nope"]),
        3,
        "not_found",
    );
    assert!(message.contains("nope"), "{message}");
}

struct Seeded {
    tasks: Vec<String>,
    code: PathBuf,
}

/// Seeds two projects, one archived, with features, one of them empty, tasks in every
/// status, tags, a body and notes of every kind.
fn seed_export(sandbox: &Sandbox) -> Seeded {
    let code = sandbox.dir("code/web");
    let path = real(&code);
    project(
        sandbox,
        &["web", "--path", &path, "--desc", "Frontend\nsecond line"],
    );
    project(sandbox, &["old"]);
    let tasks = vec![
        add(
            sandbox,
            &[
                "Fix token refresh race",
                "-p",
                "web",
                "-f",
                "auth",
                "--pri",
                "1",
                "--tag",
                "ui",
                "--tag",
                "bug",
                "--body",
                "Steps:\n1. log in",
            ],
        ),
        add(sandbox, &["Add SSO login", "-p", "web", "-f", "auth"]),
        add(sandbox, &["Paginate /orders", "-p", "web", "-f", "api"]),
        add(sandbox, &["Clean up README", "-p", "web", "--pri", "3"]),
        add(sandbox, &["Moved away", "-p", "web", "-f", "empty"]),
        add(sandbox, &["Legacy cleanup", "-p", "old", "-f", "infra"]),
    ];
    sandbox.ok(&["edit", &tasks[4], "--no-feature"]);
    sandbox.ok(&["note", &tasks[0], "seen in production"]);
    sandbox.ok(&["block", &tasks[2], "waiting on schema"]);
    sandbox.ok(&["done", &tasks[1], "shipped"]);
    sandbox.ok(&["drop", &tasks[3], "not needed"]);
    sandbox.ok(&["done", &tasks[5]]);
    sandbox.ok(&["project", "archive", "old"]);
    Seeded { tasks, code }
}

/// The export entry of a task: the `show` task with its notes.
fn exported_task(sandbox: &Sandbox, id: &str) -> Value {
    let show = sandbox.ok(&["show", id]);
    let mut task = show["task"].clone();
    task["notes"] = show["notes"].clone();
    task
}

/// The export entry of a project, built from `project show`, `show` and the stored features.
fn exported_project(sandbox: &Sandbox, name: &str, ids: &[&String]) -> Value {
    let shown = sandbox.ok(&["project", "show", name])["project"].clone();
    let features: Vec<Value> = sandbox
        .store()
        .read(|tx| {
            let project = tx.project_by_name(name)?.expect("project exists");
            tx.features(project.id)
        })
        .expect("read features")
        .into_iter()
        .map(|feature| json!({"name": feature.name, "created_at": feature.created_at}))
        .collect();
    json!({
        "name": shown["name"],
        "path": shown["path"],
        "description": shown["description"],
        "archived": shown["archived"],
        "created_at": shown["created_at"],
        "features": features,
        "tasks": ids.iter().map(|id| exported_task(sandbox, id)).collect::<Vec<_>>(),
    })
}

fn check_document(document: &Value, projects: &[Value]) {
    assert_eq!(document["version"], 1, "{document}");
    let exported_at = document["exported_at"].as_str().expect("exported_at");
    assert!(
        exported_at.ends_with('Z') && exported_at.len() == 24,
        "{exported_at}"
    );
    assert_eq!(
        document.as_object().map(serde_json::Map::len),
        Some(3),
        "{document}"
    );
    assert_eq!(document["projects"], Value::Array(projects.to_vec()));
}

#[test]
fn export_json_contains_every_project_feature_task_tag_and_note() {
    let sandbox = Sandbox::new();
    let seeded = seed_export(&sandbox);
    let t = &seeded.tasks;
    let expected = [
        exported_project(&sandbox, "old", &[&t[5]]),
        exported_project(&sandbox, "web", &[&t[0], &t[1], &t[2], &t[3], &t[4]]),
    ];
    // The seed covers what the export has to carry.
    assert_eq!(expected[0]["archived"], true);
    assert_eq!(expected[1]["tasks"][0]["tags"], json!(["bug", "ui"]));
    assert_eq!(expected[1]["features"].as_array().map(Vec::len), Some(3));
    assert_eq!(expected[1]["tasks"][1]["status"], "done");
    assert_eq!(expected[1]["tasks"][3]["status"], "dropped");

    let text = stdout(&sandbox.tk().arg("export").output().expect("run tk"));
    let document: Value = serde_json::from_str(&text).expect("the export is JSON");
    check_document(&document, &expected);

    let explicit = stdout(
        &sandbox
            .tk()
            .args(["export", "--format", "json"])
            .output()
            .expect("run tk"),
    );
    check_document(
        &serde_json::from_str(&explicit).expect("the export is JSON"),
        &expected,
    );

    let data = ok_data(&json_run(&sandbox, &["export"]));
    check_document(&data, &expected);
}

#[test]
fn export_is_restricted_by_the_flag_only() {
    let sandbox = Sandbox::new();
    let seeded = seed_export(&sandbox);
    let t = &seeded.tasks;
    let web = exported_project(&sandbox, "web", &[&t[0], &t[1], &t[2], &t[3], &t[4]]);
    let old = exported_project(&sandbox, "old", &[&t[5]]);

    let data = ok_data(&json_run(&sandbox, &["export", "-p", "web"]));
    check_document(&data, std::slice::from_ref(&web));
    let data = ok_data(&json_run(&sandbox, &["export", "-p", "old"]));
    check_document(&data, std::slice::from_ref(&old));

    let data = ok_data(
        &sandbox
            .tk()
            .args(["--json", "export"])
            .env("TASKIST_PROJECT", "web")
            .current_dir(&seeded.code)
            .output()
            .expect("run tk"),
    );
    check_document(&data, &[old, web]);

    err_message(
        &json_run(&sandbox, &["export", "-p", "nope"]),
        3,
        "not_found",
    );
    err_message(
        &json_run(&sandbox, &["export", "-p", "BAD_NAME"]),
        2,
        "usage",
    );
    err_message(
        &json_run(&sandbox, &["export", "--format", "xml"]),
        2,
        "usage",
    );
    err_message(
        &json_run(&sandbox, &["export", "--all-projects"]),
        2,
        "usage",
    );
}

#[test]
fn export_md_lists_every_task_under_its_project_and_feature() {
    let sandbox = Sandbox::new();
    let seeded = seed_export(&sandbox);
    let t = &seeded.tasks;
    let text = stdout(
        &sandbox
            .tk()
            .args(["export", "--format", "md"])
            .output()
            .expect("run tk"),
    );
    // The two runs differ only in the export time.
    let data = ok_data(&json_run(&sandbox, &["export", "--format", "md"]));
    let without_time = |text: &str| {
        let (head, rest) = text.split_once("Exported at ").expect("export time");
        let (_, tail) = rest.split_once('\n').expect("line end");
        format!("{head}{tail}")
    };
    assert_eq!(
        data.as_object().map(serde_json::Map::len),
        Some(1),
        "{data}"
    );
    assert_eq!(
        without_time(data["text"].as_str().expect("text")),
        without_time(&text)
    );

    let position = |needle: &str| {
        let found = text.find(needle);
        assert!(found.is_some(), "{needle:?} missing from:\n{text}");
        found.expect("found")
    };
    let old = position("\n## old\n");
    let web = position("\n## web\n");
    assert!(old < web, "{text}");
    let web_text = &text[web..];
    let in_web = |needle: &str| {
        let found = web_text.find(needle);
        assert!(found.is_some(), "{needle:?} missing from:\n{web_text}");
        found.expect("found")
    };
    let checkbox = |closed: bool| if closed { "[x]" } else { "[ ]" };
    let line = |id: &str, closed: bool| format!("- {} #{id} ", checkbox(closed));
    let api = in_web("\n### api\n");
    let auth = in_web("\n### auth\n");
    let empty = in_web("\n### empty\n");
    let none = in_web("\n### (no feature)\n");
    assert!(api < auth && auth < empty && empty < none, "{web_text}");
    let groups = [
        (&t[0], (auth, empty), false),
        (&t[1], (auth, empty), true),
        (&t[2], (api, auth), false),
        (&t[3], (none, web_text.len()), true),
        (&t[4], (none, web_text.len()), false),
    ];
    for (id, (start, end), closed) in groups {
        let at = in_web(&line(id, closed));
        assert!(start < at && at < end, "#{id}: {web_text}");
    }
    let legacy = position(&line(&t[5], true));
    let infra = position("\n### infra\n");
    assert!(old < infra && infra < legacy && legacy < web, "{text}");
    for title in [
        "Fix token refresh race",
        "Add SSO login",
        "Paginate /orders",
        "Clean up README",
        "Moved away",
        "Legacy cleanup",
        "seen in production",
        "waiting on schema",
        "shipped",
        "not needed",
        "Steps:",
        "1. log in",
        "bug, ui",
        "Frontend",
        "second line",
    ] {
        position(title);
    }
}

#[test]
fn export_md_keeps_stored_text_inside_its_item_at_every_line_terminator() {
    for line_break in BREAKS {
        let sandbox = Sandbox::new();
        let desc = ["about", "# forged project"].join(line_break);
        project(&sandbox, &["web", "--desc", &desc]);
        let body = ["x", "### forged feature"].join(line_break);
        let id = add(
            &sandbox,
            &["Paginate /orders", "-p", "web", "--body", &body],
        );
        let note = ["y", "- forged item", "## forged section"].join(line_break);
        sandbox.ok(&["note", &id, &note]);
        let text = stdout(
            &sandbox
                .tk()
                .args(["export", "--format", "md"])
                .output()
                .expect("run tk"),
        );
        let forged: Vec<&str> = top_level_blocks(&text)
            .into_iter()
            .filter(|line| line.contains("forged"))
            .collect();
        assert!(forged.is_empty(), "{line_break:?}: {forged:?} in {text:?}");
        for continuation in [
            "\n- description: about\n  # forged project\n",
            "  - body: x\n    ### forged feature\n",
            ": y\n    - forged item\n    ## forged section\n",
        ] {
            assert!(
                text.contains(continuation),
                "{line_break:?}: {continuation:?} missing from {text:?}"
            );
        }
    }
}

fn write_file(sandbox: &Sandbox, name: &str, text: &str) -> String {
    let path = sandbox.root().join(name);
    std::fs::write(&path, text).expect("write import file");
    path.to_str().expect("utf-8 path").to_owned()
}

#[test]
fn import_creates_one_task_per_line_in_file_order() {
    let sandbox = Sandbox::new();
    project(&sandbox, &["web"]);
    project(&sandbox, &["api"]);
    let file = write_file(
        &sandbox,
        "tasks.jsonl",
        concat!(
            r#"{"title": "  First  ", "project": "web", "feature": "auth", "pri": 1, "tags": ["ui", "bug"], "body": "line one\nline two", "by": "agent:x"}"#,
            "\n\n   \n",
            r#"{"title": "Second", "project": "api"}"#,
            "\r\n",
            r#"{"title": "Third", "project": "web", "feature": "auth", "tags": []}"#,
        ),
    );
    let data = ok_data(
        &sandbox
            .tk()
            .args(["--json", "--by", "cli", "import", &file])
            .output()
            .expect("run tk"),
    );
    assert_eq!(data, json!({"created": [1, 2, 3]}));

    let first = sandbox.ok(&["show", "1"])["task"].clone();
    assert_eq!(first["title"], "First");
    assert_eq!(first["project"], "web");
    assert_eq!(first["feature"], "auth");
    assert_eq!(first["priority"], 1);
    assert_eq!(first["tags"], json!(["bug", "ui"]));
    assert_eq!(first["body"], "line one\nline two");
    assert_eq!(first["created_by"], "agent:x");
    assert_eq!(first["status"], "todo");
    let second = sandbox.ok(&["show", "2"])["task"].clone();
    assert_eq!(second["project"], "api");
    assert_eq!(second["feature"], Value::Null);
    assert_eq!(second["priority"], 2);
    assert_eq!(second["body"], "");
    assert_eq!(second["created_by"], "cli");
    let third = sandbox.ok(&["show", "3"])["task"].clone();
    assert_eq!(third["feature"], "auth");
    assert_eq!(task_count(&sandbox), 3);

    let human = stdout(
        &sandbox
            .tk()
            .args(["import", &file])
            .output()
            .expect("run tk"),
    );
    assert_eq!(human, "imported 3 tasks: #4, #5, #6\n");
}

#[test]
fn import_reads_stdin_and_uses_the_resolved_scope() {
    let sandbox = Sandbox::new();
    project(&sandbox, &["web"]);
    project(&sandbox, &["api"]);
    let data = ok_data(
        &sandbox
            .tk()
            .args(["--json", "import", "-", "-p", "web"])
            .write_stdin(
                "{\"title\": \"From stdin\"}\n{\"title\": \"Elsewhere\", \"project\": \"api\"}\n",
            )
            .output()
            .expect("run tk"),
    );
    assert_eq!(data, json!({"created": [1, 2]}));
    assert_eq!(sandbox.ok(&["show", "1"])["task"]["project"], "web");
    assert_eq!(sandbox.ok(&["show", "2"])["task"]["project"], "api");

    let data = ok_data(
        &sandbox
            .tk()
            .args(["--json", "import", "-"])
            .env("TASKIST_PROJECT", "api")
            .write_stdin("{\"title\": \"From the variable\"}\n")
            .output()
            .expect("run tk"),
    );
    assert_eq!(data, json!({"created": [3]}));
    assert_eq!(sandbox.ok(&["show", "3"])["task"]["project"], "api");

    let dir = sandbox.dir("code/web");
    sandbox.ok(&["project", "edit", "web", "--path", &real(&dir)]);
    let data = ok_data(
        &sandbox
            .tk()
            .args(["--json", "import", "-"])
            .current_dir(&dir)
            .write_stdin("{\"title\": \"From the directory\"}\n")
            .output()
            .expect("run tk"),
    );
    assert_eq!(data, json!({"created": [4]}));
    assert_eq!(sandbox.ok(&["show", "4"])["task"]["project"], "web");
}

#[test]
fn import_without_a_resolved_project_names_the_flag() {
    let sandbox = Sandbox::new();
    project(&sandbox, &["web"]);
    let output = sandbox
        .tk()
        .args(["--json", "import", "-"])
        .write_stdin("{\"title\": \"Fine\", \"project\": \"web\"}\n{\"title\": \"No project\"}\n")
        .output()
        .expect("run tk");
    let message = err_message(&output, 2, "usage");
    assert!(message.contains("line 2"), "{message}");
    assert!(message.contains("-p/--project"), "{message}");
    assert_eq!(task_count(&sandbox), 0);
}

/// The line numbers a message names: every number that follows the word `line`.
fn line_numbers(message: &str) -> Vec<&str> {
    message
        .split("line ")
        .skip(1)
        .map(|rest| {
            rest.split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap_or("")
        })
        .collect()
}

#[test]
fn an_import_error_names_only_the_line_of_the_file() {
    assert_eq!(line_numbers("line 3: x at line 1 column 2"), ["3", "1"]);
    let cases = [
        ("{\"title\": \"Good\"}\n\n{\"title\": \"Broken\"\n", "3"),
        (
            "{\"title\": \"Good\"}\n{\"title\": \"Extra\", \"owner\": \"x\"}\n",
            "2",
        ),
        ("{\"title\": \"Good\"}\n{\"pri\": 1}\n", "2"),
    ];
    for (text, line) in cases {
        let sandbox = Sandbox::new();
        project(&sandbox, &["web"]);
        let file = write_file(&sandbox, "tasks.jsonl", text);
        let output = json_run(&sandbox, &["import", &file, "-p", "web"]);
        let message = err_message(&output, 2, "usage");
        assert!(message.starts_with(&format!("line {line}: ")), "{message}");
        assert_eq!(line_numbers(&message), [line], "{message}");
        assert_eq!(task_count(&sandbox), 0);
    }
}

#[test]
fn a_malformed_import_is_refused_before_the_database_is_opened() {
    let sandbox = Sandbox::new();
    let file = write_file(
        &sandbox,
        "tasks.jsonl",
        "{\"title\": \"Good\"}\n{\"title\": \"Broken\"\n",
    );
    // Calibration: the sandbox starts without a database file.
    assert!(!sandbox.db().exists());
    let output = json_run(&sandbox, &["import", &file, "-p", "web"]);
    let message = err_message(&output, 2, "usage");
    assert!(message.starts_with("line 2: "), "{message}");
    assert!(!sandbox.db().exists(), "the import created the database");
}

#[test]
fn a_malformed_import_is_a_usage_error_on_a_database_that_cannot_be_opened() {
    let sandbox = Sandbox::new();
    let file = write_file(
        &sandbox,
        "tasks.jsonl",
        "{\"title\": \"Good\"}\n{\"title\": \"Broken\"\n",
    );
    project(&sandbox, &["web"]);
    sandbox.sql("PRAGMA user_version = 99", []);
    // Calibration: the database is refused once it is opened.
    err_message(
        &json_run(&sandbox, &["ls", "-p", "web"]),
        1,
        "unsupported_schema",
    );
    let output = json_run(&sandbox, &["import", &file, "-p", "web"]);
    let message = err_message(&output, 2, "usage");
    assert!(message.starts_with("line 2: "), "{message}");
}

#[test]
fn import_refuses_an_unknown_project_flag_or_variable() {
    let sandbox = Sandbox::new();
    project(&sandbox, &["web"]);
    let file = write_file(
        &sandbox,
        "tasks.jsonl",
        "{\"title\": \"Good\", \"project\": \"web\"}\n",
    );
    let output = json_run(&sandbox, &["import", &file, "-p", "nosuch"]);
    let message = err_message(&output, 3, "not_found");
    assert!(message.contains("nosuch"), "{message}");
    assert_eq!(task_count(&sandbox), 0);

    let output = sandbox
        .tk()
        .args(["--json", "import", &file])
        .env("TASKIST_PROJECT", "nosuch")
        .output()
        .expect("run tk");
    let message = err_message(&output, 3, "not_found");
    assert!(message.contains("nosuch"), "{message}");
    assert_eq!(task_count(&sandbox), 0);
}

#[test]
fn a_bad_line_fails_the_whole_import() {
    let cases = [
        ("{\"title\": \"Broken\"", 2, "usage"),
        ("{\"title\": \"Extra\", \"owner\": \"x\"}", 2, "usage"),
        ("{\"project\": \"web\"}", 2, "usage"),
        (
            "{\"title\": \"Elsewhere\", \"project\": \"nope\"}",
            3,
            "not_found",
        ),
        ("{\"title\": \"   \"}", 2, "usage"),
        ("{\"title\": \"Urgent\", \"pri\": 4}", 2, "usage"),
        (
            "{\"title\": \"Tagged\", \"tags\": [\"Bad Tag\"]}",
            2,
            "usage",
        ),
        (
            "{\"title\": \"Featured\", \"feature\": \"BAD\"}",
            2,
            "usage",
        ),
        ("{\"title\": \"Old\", \"project\": \"old\"}", 4, "conflict"),
        ("[1, 2]", 2, "usage"),
    ];
    for (bad, exit, code) in cases {
        let sandbox = Sandbox::new();
        project(&sandbox, &["web"]);
        project(&sandbox, &["old"]);
        sandbox.ok(&["project", "archive", "old"]);
        let file = write_file(
            &sandbox,
            "tasks.jsonl",
            &format!("{{\"title\": \"Good\"}}\n\n{bad}\n{{\"title\": \"Also good\"}}\n"),
        );
        let output = json_run(&sandbox, &["import", &file, "-p", "web"]);
        let message = err_message(&output, exit, code);
        assert!(message.starts_with("line 3: "), "{bad}: {message}");
        assert_eq!(task_count(&sandbox), 0, "{bad}");
    }
}

#[test]
fn import_reports_an_invalid_utf8_line_and_a_missing_file() {
    let sandbox = Sandbox::new();
    project(&sandbox, &["web"]);
    let path = sandbox.root().join("bad.jsonl");
    std::fs::write(&path, b"{\"title\": \"Good\"}\n{\"title\": \"\xff\"}\n").expect("write");
    let output = json_run(
        &sandbox,
        &["import", path.to_str().expect("utf-8"), "-p", "web"],
    );
    let message = err_message(&output, 2, "usage");
    assert!(message.starts_with("line 2: "), "{message}");
    assert_eq!(task_count(&sandbox), 0);

    let missing = sandbox.root().join("missing.jsonl");
    let output = json_run(&sandbox, &["import", missing.to_str().expect("utf-8")]);
    let message = err_message(&output, 1, "internal");
    assert!(message.contains("missing.jsonl"), "{message}");
}

#[test]
fn an_empty_import_creates_nothing() {
    let sandbox = Sandbox::new();
    let data = ok_data(
        &sandbox
            .tk()
            .args(["--json", "import", "-"])
            .write_stdin("\n  \n")
            .output()
            .expect("run tk"),
    );
    assert_eq!(data, json!({"created": []}));
    let human = stdout(&sandbox.tk().args(["import", "-"]).output().expect("run tk"));
    assert_eq!(human, "imported 0 tasks\n");
}
