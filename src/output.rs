//! Rendering of results and errors, and the only place that writes to stdout or stderr.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::Write;

use anstyle::{AnsiColor, Style};
use serde::Serialize;

use crate::command::exchange::{Brief, Export, Imported};
use crate::command::feature::{FeatureList, FeatureMove};
use crate::command::project::{
    ProjectArchive, ProjectData, ProjectList, ProjectRemoval, ProjectShow,
};
use crate::command::task::{NextTask, TaskData, TaskList, TaskShow};
use crate::command::transition::TransitionData;
use crate::command::{FeatureView, ProjectView, TaskView};
use crate::env::Env;
use crate::error::Error;
use crate::model::Status;

/// How results and errors are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable text.
    Text,
    /// One JSON envelope per invocation.
    Json,
}

impl Format {
    /// Selects the format: `--json`, else `TASKIST_FORMAT` (`json` or `text`), else text.
    pub fn detect(json_flag: bool, taskist_format: Option<&OsStr>) -> Result<Self, Error> {
        if json_flag {
            return Ok(Self::Json);
        }
        match taskist_format {
            None => Ok(Self::Text),
            Some(value) if value == "json" => Ok(Self::Json),
            Some(value) if value == "text" => Ok(Self::Text),
            Some(value) => Err(Error::Usage(format!(
                "TASKIST_FORMAT must be json or text, got {:?}",
                value.to_string_lossy()
            ))),
        }
    }

    /// Selects the format from the `--json` flag and the captured environment.
    pub fn select(json_flag: bool, env: &Env) -> Result<Self, Error> {
        Self::detect(json_flag, env.var("TASKIST_FORMAT"))
    }
}

#[derive(Serialize)]
struct Success<'a, T> {
    ok: bool,
    data: &'a T,
}

#[derive(Serialize)]
struct Failure<'a> {
    ok: bool,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: String,
}

/// Renders the JSON success envelope `{"ok":true,"data":...}` as one line.
pub fn render_success<T: Serialize>(data: &T) -> Result<String, Error> {
    encode_line(&Success { ok: true, data })
}

/// Renders an error: the JSON failure envelope as one line, or `error: <message>`.
pub fn render_failure(format: Format, error: &Error) -> Result<String, Error> {
    match format {
        Format::Text => Ok(format!("error: {error}\n")),
        Format::Json => encode_line(&Failure {
            ok: false,
            error: ErrorBody {
                code: error.code(),
                message: error.to_string(),
            },
        }),
    }
}

fn encode_line(value: &impl Serialize) -> Result<String, Error> {
    let mut line = serde_json::to_string(value)
        .map_err(|err| Error::Internal(format!("cannot encode the output: {err}")))?;
    line.push('\n');
    Ok(line)
}

/// Styles human text: the status word and headings, and only when colour is enabled.
///
/// Only the styles added here are affected; stored text is always written as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paint {
    colour: bool,
}

/// The width of the longest status word, to which task lines pad the status column.
const STATUS_WIDTH: usize = 7;

impl Paint {
    /// A painter that styles when `colour` is true and leaves text plain otherwise.
    pub const fn new(colour: bool) -> Self {
        Self { colour }
    }

    fn styled(self, style: Style, text: &str) -> String {
        if self.colour {
            format!("{style}{text}{style:#}")
        } else {
            text.to_owned()
        }
    }

    /// A heading: a project name, a feature group, a task title line.
    fn heading(self, text: &str) -> String {
        self.styled(Style::new().bold(), text)
    }

    /// The status word.
    fn status(self, status: Status) -> String {
        let colour = match status {
            Status::Todo => AnsiColor::Cyan,
            Status::Doing => AnsiColor::Yellow,
            Status::Blocked => AnsiColor::Red,
            Status::Done => AnsiColor::Green,
            Status::Dropped => AnsiColor::BrightBlack,
        };
        self.styled(colour.on_default(), status.as_str())
    }

    /// The status word padded to the width of the status column.
    fn status_column(self, status: Status) -> String {
        let padding = STATUS_WIDTH.saturating_sub(status.as_str().len());
        format!("{}{}", self.status(status), " ".repeat(padding))
    }
}

/// Writes plain text to stdout, as for `--help` and `--version`, and returns exit code 0.
pub fn emit_text(text: &str) -> u8 {
    match write_stdout(text) {
        Ok(()) => 0,
        Err(err) => emit_failure(Format::Text, &err),
    }
}

/// Writes a success result to stdout and returns exit code 0.
///
/// `human` is the text form; JSON output wraps `data` in the success envelope.
pub fn emit_success<T: Serialize>(format: Format, data: &T, human: &str) -> u8 {
    let text = match format {
        Format::Text => Ok(human.to_owned()),
        Format::Json => render_success(data),
    };
    match text.and_then(|text| write_stdout(&text)) {
        Ok(()) => 0,
        Err(err) => emit_failure(format, &err),
    }
}

/// Writes a command result: the success through [`emit_success`], with `human` rendering
/// its text form when the format is text, or the error through [`emit_failure`]; returns
/// the exit code.
pub fn emit<T: Serialize>(
    format: Format,
    result: Result<T, Error>,
    human: impl FnOnce(&T) -> String,
) -> u8 {
    match result {
        Ok(data) => {
            let text = match format {
                Format::Text => human(&data),
                Format::Json => String::new(),
            };
            emit_success(format, &data, &text)
        }
        Err(err) => emit_failure(format, &err),
    }
}

fn plural(count: i64, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn project_line(project: &ProjectView) -> String {
    let archived = if project.archived { "  archived" } else { "" };
    format!("{}  ({} open){archived}\n", project.name, project.open)
}

/// The task id, the priority `P0`..`P3`, the padded status and the title, without
/// indentation or line end.
fn task_line(task: &TaskView, paint: Paint) -> String {
    format!(
        "#{}  P{}  {}  {}",
        task.id,
        task.priority,
        paint.status_column(task.status),
        task.title
    )
}

/// `project` or `project/feature`.
fn location(task: &TaskView) -> String {
    task.feature.as_ref().map_or_else(
        || task.project.clone(),
        |feature| format!("{}/{feature}", task.project),
    )
}

/// The human form of `tk ls` and `tk find`, grouped by project and feature.
///
/// A `name  (N open)` heading per project, a heading per feature with `-` for tasks
/// without one, and one line per task; a blocked task ends with its latest reason in
/// parentheses.
pub fn task_list_text(list: &TaskList, paint: Paint) -> String {
    let mut projects: Vec<&str> = list
        .tasks
        .iter()
        .map(|task| task.project.as_str())
        .collect();
    if let Some(project) = &list.scope.project {
        projects.push(&project.name);
    }
    projects.sort_unstable();
    projects.dedup();
    if projects.is_empty() {
        return "no tasks\n".to_owned();
    }
    let mut text = String::new();
    for project in projects {
        let open = list.open.get(project).copied().unwrap_or(0);
        let _ = writeln!(text, "{}  ({open} open)", paint.heading(project));
        let mut group: Option<Option<&str>> = None;
        for task in list.tasks.iter().filter(|task| task.project == project) {
            let feature = task.feature.as_deref();
            if group != Some(feature) {
                let _ = writeln!(text, "  {}", paint.heading(feature.unwrap_or("-")));
                group = Some(feature);
            }
            let _ = write!(text, "    {}", task_line(task, paint));
            if let Some(reason) = list.reasons.get(&task.id) {
                let _ = write!(text, "  ({reason})");
            }
            text.push('\n');
        }
    }
    text
}

/// The human form of `tk brief`: the digest itself.
pub fn brief_text(brief: &Brief) -> String {
    brief.text.clone()
}

/// The human form of `tk export`: the document itself.
pub fn export_text(export: &Export) -> String {
    export.text().to_owned()
}

/// The human form of `tk import`: the number of created tasks and their ids.
pub fn import_text(imported: &Imported) -> String {
    let count = plural(
        i64::try_from(imported.ids.len()).unwrap_or(i64::MAX),
        "task",
    );
    if imported.ids.is_empty() {
        return format!("imported {count}\n");
    }
    let ids: Vec<String> = imported.ids.iter().map(|id| format!("#{id}")).collect();
    format!("imported {count}: {}\n", ids.join(", "))
}

/// The human form of `tk show`: a heading, one `name: value` line per field, the body
/// after a blank line, and the notes oldest first.
pub fn task_show_text(show: &TaskShow, paint: Paint) -> String {
    let task = &show.task;
    let mut text = paint.heading(&format!("#{}  {}", task.id, task.title));
    let tags = if task.tags.is_empty() {
        "-".to_owned()
    } else {
        task.tags.join(", ")
    };
    let _ = write!(
        text,
        "\nproject:  {}\nfeature:  {}\nstatus:   {}\npriority: P{}\ntags:     {tags}\n\
         created:  {} by {}\nupdated:  {}\n",
        task.project,
        task.feature.as_deref().unwrap_or("-"),
        paint.status(task.status),
        task.priority,
        task.created_at,
        task.created_by,
        task.updated_at,
    );
    if let Some(closed_at) = &task.closed_at {
        let _ = writeln!(text, "closed:   {closed_at}");
    }
    if !task.body.is_empty() {
        let _ = write!(text, "\n{}", task.body);
        if !task.body.ends_with('\n') {
            text.push('\n');
        }
    }
    if !show.notes.is_empty() {
        let _ = writeln!(text, "\n{}", paint.heading("notes:"));
        for note in &show.notes {
            let _ = writeln!(
                text,
                "  {}  {}  {}: {}",
                note.created_at,
                note.kind.as_str(),
                note.author,
                note.text
            );
        }
    }
    text
}

/// The human form of `tk add`.
pub fn task_added_text(data: &TaskData) -> String {
    format!(
        "added #{} to {}: {}\n",
        data.task.id,
        location(&data.task),
        data.task.title
    )
}

/// The human form of `tk edit`.
pub fn task_updated_text(data: &TaskData) -> String {
    format!(
        "updated #{} in {}: {}\n",
        data.task.id,
        location(&data.task),
        data.task.title
    )
}

/// The human form of the status commands: one line per task saying whether it changed.
pub fn transition_text(data: &TransitionData) -> String {
    let mut text = String::new();
    for entry in &data.tasks {
        let state = if entry.changed { "now" } else { "already" };
        let _ = writeln!(
            text,
            "#{} is {state} {}: {}",
            entry.task.id, entry.task.status, entry.task.title
        );
    }
    text
}

/// The human form of `tk note`.
pub fn task_noted_text(data: &TaskData) -> String {
    format!(
        "noted #{} in {}: {}\n",
        data.task.id,
        location(&data.task),
        data.task.title
    )
}

/// The human form of `tk next`: the task line with its location, or `no open task`.
pub fn next_task_text(next: &NextTask, paint: Paint) -> String {
    next.task.as_ref().map_or_else(
        || "no open task\n".to_owned(),
        |task| format!("{}  ({})\n", task_line(task, paint), location(task)),
    )
}

/// The human form of `project ls`: one `name  (N open)` line per project, or `no projects`.
pub fn project_list_text(list: &ProjectList) -> String {
    if list.projects.is_empty() {
        return "no projects\n".to_owned();
    }
    list.projects.iter().map(project_line).collect()
}

/// The human form of `project add`.
pub fn project_added_text(data: &ProjectData) -> String {
    format!("added project {}\n", data.project.name)
}

/// The human form of `project edit`.
pub fn project_updated_text(data: &ProjectData) -> String {
    format!("updated project {}\n", data.project.name)
}

/// The human form of `project show`.
pub fn project_show_text(show: &ProjectShow) -> String {
    let project = &show.project;
    let counts = &show.counts;
    let mut text = project_line(project);
    let _ = writeln!(
        text,
        "path: {}\ndescription: {}",
        project.path.as_deref().unwrap_or("-"),
        project.description
    );
    let _ = writeln!(
        text,
        "tasks: todo {}, doing {}, blocked {}, done {}, dropped {}",
        counts.todo, counts.doing, counts.blocked, counts.done, counts.dropped
    );
    if show.features.is_empty() {
        text.push_str("features: none\n");
    } else {
        text.push_str("features:\n");
        show.features
            .iter()
            .for_each(|feature| text.push_str(&feature_line(feature)));
    }
    text
}

/// The human form of `project archive`.
pub fn project_archive_text(data: &ProjectArchive) -> String {
    let name = &data.project.name;
    match (data.changed, data.project.archived) {
        (true, true) => format!("archived project {name}\n"),
        (true, false) => format!("unarchived project {name}\n"),
        (false, true) => format!("project {name} is already archived\n"),
        (false, false) => format!("project {name} is not archived\n"),
    }
}

/// The human form of `project rm`.
pub fn project_removal_text(data: &ProjectRemoval) -> String {
    format!(
        "removed project {} and {}\n",
        data.removed,
        plural(data.tasks, "task")
    )
}

fn feature_line(feature: &FeatureView) -> String {
    format!(
        "  {}  {} open / {} total\n",
        feature.name, feature.open, feature.total
    )
}

/// The human form of `feature ls`: the features under a heading per project.
pub fn feature_list_text(list: &FeatureList) -> String {
    let mut text = String::new();
    let mut heading: Option<&str> = None;
    for feature in &list.features {
        if heading != Some(feature.project.as_str()) {
            let _ = writeln!(text, "{}", feature.project);
            heading = Some(&feature.project);
        }
        text.push_str(&feature_line(feature));
    }
    match (&list.scope.project, text.is_empty()) {
        (Some(project), true) => format!("{}\n  no features\n", project.name),
        (None, true) => "no features\n".to_owned(),
        (_, false) => text,
    }
}

/// The human form of `feature mv`.
pub fn feature_move_text(data: &FeatureMove) -> String {
    let feature = &data.feature;
    if data.merged {
        format!(
            "merged feature {} into {} in {} ({} moved)\n",
            data.from,
            feature.name,
            feature.project,
            plural(data.moved, "task")
        )
    } else {
        format!(
            "renamed feature {} to {} in {} ({})\n",
            data.from,
            feature.name,
            feature.project,
            plural(data.moved, "task")
        )
    }
}

/// Writes an error to stderr and returns its exit code.
pub fn emit_failure(format: Format, error: &Error) -> u8 {
    // A failure to report the error has nowhere left to go; the exit code still tells.
    let _ =
        render_failure(format, error).and_then(|text| write_all(std::io::stderr().lock(), &text));
    error.exit_code()
}

/// Writes to stdout through `anstream`, which passes the text through as it is: the
/// styles in it are those [`Paint`] added when [`Env::colour`](crate::env::Env::colour),
/// which applies the automatic rules of `anstream` to the captured environment, enables
/// colour, and escape sequences in stored text are printed as stored.
fn write_stdout(text: &str) -> Result<(), Error> {
    write_all(
        anstream::AutoStream::new(std::io::stdout().lock(), anstream::ColorChoice::Always),
        text,
    )
}

fn write_all(mut stream: impl Write, text: &str) -> Result<(), Error> {
    stream
        .write_all(text.as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|err| Error::Internal(format!("cannot write output: {err}")))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::PathBuf;

    use serde_json::{Value, json};

    use super::{Format, plural, project_list_text, render_failure, render_success};
    use crate::command::ProjectView;
    use crate::command::project::ProjectList;
    use crate::env::Env;
    use crate::error::Error;

    #[test]
    fn json_flag_wins_over_the_variable() {
        assert_eq!(
            Format::detect(true, Some(OsStr::new("text"))).unwrap(),
            Format::Json
        );
        assert_eq!(
            Format::detect(true, Some(OsStr::new("xml"))).unwrap(),
            Format::Json
        );
    }

    #[test]
    fn variable_selects_json_or_text_and_defaults_to_text() {
        assert_eq!(Format::detect(false, None).unwrap(), Format::Text);
        assert_eq!(
            Format::detect(false, Some(OsStr::new("json"))).unwrap(),
            Format::Json
        );
        assert_eq!(
            Format::detect(false, Some(OsStr::new("text"))).unwrap(),
            Format::Text
        );
    }

    #[test]
    fn other_variable_values_are_usage_errors() {
        for value in ["xml", "", "JSON"] {
            let err = Format::detect(false, Some(OsStr::new(value))).unwrap_err();
            assert!(matches!(err, Error::Usage(_)), "{value}: {err:?}");
            assert!(err.to_string().contains("TASKIST_FORMAT"), "{err}");
        }
    }

    #[test]
    fn select_reads_the_captured_variable() {
        let env = Env::new(
            [("TASKIST_FORMAT".into(), "json".into())],
            PathBuf::from("/"),
            false,
        );
        assert_eq!(Format::select(false, &env).unwrap(), Format::Json);
    }

    #[test]
    fn success_envelope_is_one_line() {
        let line = render_success(&json!({"id": 1, "title": "a\nb"})).unwrap();
        assert_eq!(
            line,
            "{\"ok\":true,\"data\":{\"id\":1,\"title\":\"a\\nb\"}}\n"
        );
    }

    #[test]
    fn failure_envelope_is_one_line_with_code_and_message() {
        let line =
            render_failure(Format::Json, &Error::NotFound("no task 7\nsee ls".into())).unwrap();
        assert_eq!(line.lines().count(), 1);
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            value,
            json!({"ok": false, "error": {"code": "not_found", "message": "no task 7\nsee ls"}})
        );
    }

    #[test]
    fn human_failure_is_prefixed_with_error() {
        assert_eq!(
            render_failure(Format::Text, &Error::Conflict("project exists".into())).unwrap(),
            "error: project exists\n"
        );
    }

    #[test]
    fn project_list_text_names_one_project_per_line() {
        assert_eq!(
            project_list_text(&ProjectList { projects: vec![] }),
            "no projects\n"
        );
        let project = |name: &str, archived: bool, open: i64| ProjectView {
            name: name.into(),
            path: None,
            description: String::new(),
            archived,
            created_at: "2026-09-27T12:00:00.000Z".into(),
            open,
        };
        let list = ProjectList {
            projects: vec![project("api", false, 1), project("old", true, 0)],
        };
        assert_eq!(
            project_list_text(&list),
            "api  (1 open)\nold  (0 open)  archived\n"
        );
    }

    #[test]
    fn plural_counts() {
        assert_eq!(plural(1, "task"), "1 task");
        assert_eq!(plural(0, "task"), "0 tasks");
        assert_eq!(plural(2, "task"), "2 tasks");
    }

    fn task(
        id: i64,
        feature: Option<&str>,
        status: crate::model::Status,
    ) -> crate::command::TaskView {
        crate::command::TaskView {
            id,
            project: "web".into(),
            feature: feature.map(str::to_owned),
            title: format!("task {id}"),
            body: String::new(),
            status,
            priority: 1,
            tags: vec![],
            created_at: "2026-09-27T12:00:00.000Z".into(),
            updated_at: "2026-09-27T12:00:00.000Z".into(),
            closed_at: None,
            created_by: "me".into(),
        }
    }

    #[test]
    fn paint_styles_only_when_colour_is_enabled_and_pads_outside_the_style() {
        use crate::model::Status;
        let plain = super::Paint::new(false);
        let colour = super::Paint::new(true);
        assert_eq!(plain.status_column(Status::Todo), "todo   ");
        assert_eq!(plain.status_column(Status::Blocked), "blocked");
        assert_eq!(plain.heading("web"), "web");
        let styled = colour.status_column(Status::Doing);
        assert!(styled.starts_with("\x1b["), "{styled:?}");
        assert!(styled.ends_with("doing\x1b[0m  "), "{styled:?}");
        assert_eq!(colour.heading("web"), "\x1b[1mweb\x1b[0m");
        for status in Status::ALL {
            assert!(colour.status(status).contains(status.as_str()));
            assert_ne!(colour.status(status), plain.status(status));
        }
    }

    #[test]
    fn task_list_text_groups_features_and_shows_reasons() {
        use crate::command::task::TaskList;
        use crate::model::Status;
        use crate::scope::{Scope, Source};
        let list = TaskList {
            scope: Scope {
                project: None,
                source: Source::None,
            },
            tasks: vec![
                task(3, Some("api"), Status::Blocked),
                task(1, Some("auth"), Status::Todo),
                task(2, None, Status::Doing),
            ],
            open: std::iter::once(("web".to_owned(), 5)).collect(),
            reasons: std::iter::once((3, "waiting".to_owned())).collect(),
        };
        assert_eq!(
            super::task_list_text(&list, super::Paint::new(false)),
            "web  (5 open)\n  api\n    #3  P1  blocked  task 3  (waiting)\n  auth\n    \
             #1  P1  todo     task 1\n  -\n    #2  P1  doing    task 2\n"
        );
        let empty = TaskList {
            tasks: vec![],
            ..list
        };
        assert_eq!(
            super::task_list_text(&empty, super::Paint::new(false)),
            "no tasks\n"
        );
    }

    #[test]
    fn task_texts_of_show_add_edit_and_next() {
        use crate::command::task::{NextTask, NoteView, TaskData, TaskShow};
        use crate::model::{NoteKind, Status};
        use crate::scope::{Scope, Source};
        let mut closed = task(4, Some("auth"), Status::Done);
        closed.closed_at = Some("2026-09-28T00:00:00.000Z".into());
        closed.body = "ends with a newline\n".into();
        closed.tags = vec!["a".into(), "b".into()];
        let show = TaskShow {
            task: closed.clone(),
            notes: vec![NoteView {
                id: 1,
                kind: NoteKind::Done,
                text: "shipped".into(),
                author: "me".into(),
                created_at: "2026-09-28T00:00:00.000Z".into(),
            }],
        };
        assert_eq!(
            super::task_show_text(&show, super::Paint::new(false)),
            "#4  task 4\nproject:  web\nfeature:  auth\nstatus:   done\npriority: P1\n\
             tags:     a, b\ncreated:  2026-09-27T12:00:00.000Z by me\n\
             updated:  2026-09-27T12:00:00.000Z\nclosed:   2026-09-28T00:00:00.000Z\n\
             \nends with a newline\n\nnotes:\n  2026-09-28T00:00:00.000Z  done  me: shipped\n"
        );
        let data = TaskData { task: closed };
        assert_eq!(
            super::task_added_text(&data),
            "added #4 to web/auth: task 4\n"
        );
        assert_eq!(
            super::task_updated_text(&data),
            "updated #4 in web/auth: task 4\n"
        );
        let scope = Scope {
            project: None,
            source: Source::None,
        };
        let next = NextTask {
            scope: scope.clone(),
            task: Some(task(5, None, Status::Todo)),
        };
        assert_eq!(
            super::next_task_text(&next, super::Paint::new(false)),
            "#5  P1  todo     task 5  (web)\n"
        );
        let none = NextTask { scope, task: None };
        assert_eq!(
            super::next_task_text(&none, super::Paint::new(false)),
            "no open task\n"
        );
    }
}
