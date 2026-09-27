//! The digest and data exchange commands: `brief`, `export` and `import`.
//!
//! Export and import write only to stdout and read only the named file or stdin.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Read;
use std::path::Path;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

use super::task::{self, Creation, ListFilter, NoteView, TaskList};
use super::{TaskView, open_store, project_named};
use crate::cli::ExportFormat;
use crate::env::Env;
use crate::error::Error;
use crate::model::{
    DEFAULT_PRIORITY, Project, Status, normalize_title, validate_name, validate_priority,
};
use crate::scope::{self, Request, Scope, Target};
use crate::store::Tx;

/// `brief` data: `{scope, text}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Brief {
    /// The resolved scope.
    pub scope: Scope,
    /// The markdown digest.
    pub text: String,
}

/// `tk brief`: a markdown digest of the open tasks of the resolved scope.
///
/// Per project a `# name (N open)` heading, per feature a `## name` heading, `## (no
/// feature)` last, with one `- #id P<n> status title` line per task in `ls` order, then a
/// `Blocked:` list with the latest reason of every blocked task, its continuation lines
/// indented under the item so they cannot form a heading or item. A project without open
/// tasks is left out unless it is the scoped one.
pub fn brief(env: &Env, request: Request<'_>) -> Result<Brief, Error> {
    let list = task::list(env, request, ListFilter::default())?;
    let text = brief_markdown(&list);
    Ok(Brief {
        scope: list.scope,
        text,
    })
}

fn brief_markdown(list: &TaskList) -> String {
    let scoped = list.scope.project.as_ref().map(|project| &project.name);
    let mut sections = Vec::new();
    for (project, open) in &list.open {
        if *open == 0 && scoped != Some(project) {
            continue;
        }
        let tasks: Vec<&TaskView> = list
            .tasks
            .iter()
            .filter(|task| &task.project == project)
            .collect();
        let mut text = format!("# {project} ({open} open)\n");
        let mut group: Option<Option<&str>> = None;
        for task in &tasks {
            let feature = task.feature.as_deref();
            if group != Some(feature) {
                let _ = writeln!(text, "\n## {}", feature.unwrap_or("(no feature)"));
                group = Some(feature);
            }
            let _ = writeln!(
                text,
                "- #{} P{} {} {}",
                task.id, task.priority, task.status, task.title
            );
        }
        let blocked: Vec<&&TaskView> = tasks
            .iter()
            .filter(|task| task.status == Status::Blocked)
            .collect();
        if !blocked.is_empty() {
            text.push_str("\nBlocked:\n");
            for task in blocked {
                let reason = list
                    .reasons
                    .get(&task.id)
                    .map_or("no reason recorded", String::as_str);
                let _ = writeln!(
                    text,
                    "- #{} {}: {}",
                    task.id,
                    task.title,
                    continued(reason, "  ")
                );
            }
        }
        sections.push(text);
    }
    if sections.is_empty() {
        return "no open tasks\n".to_owned();
    }
    sections.join("\n")
}

/// The export document: `{version, exported_at, projects}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportDocument {
    /// The document format version.
    pub version: u32,
    /// Export time, UTC.
    pub exported_at: String,
    /// The exported projects, by name.
    pub projects: Vec<ExportProject>,
}

/// A project of the export document with its features and tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportProject {
    /// Unique name.
    pub name: String,
    /// Linked directory, canonical, if any.
    pub path: Option<String>,
    /// Free text.
    pub description: String,
    /// Whether the project is archived.
    pub archived: bool,
    /// Creation time, UTC.
    pub created_at: String,
    /// Its features, by name.
    pub features: Vec<ExportFeature>,
    /// Its tasks in every status, by id.
    pub tasks: Vec<ExportTask>,
}

/// A feature of the export document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportFeature {
    /// Name, unique within the project.
    pub name: String,
    /// Creation time, UTC.
    pub created_at: String,
}

/// A task of the export document: the task fields and its notes, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportTask {
    /// The task fields.
    #[serde(flatten)]
    pub task: TaskView,
    /// Its notes, oldest first.
    pub notes: Vec<NoteView>,
}

/// `export` data: the JSON document itself, or `{text}` for markdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Export {
    /// The JSON document, with its indented text form.
    Json {
        /// The document.
        document: ExportDocument,
        /// The document as indented JSON text.
        text: String,
    },
    /// The markdown rendering.
    Markdown {
        /// The markdown text.
        text: String,
    },
}

impl Export {
    /// The text the export prints in human output.
    pub fn text(&self) -> &str {
        match self {
            Self::Json { text, .. } | Self::Markdown { text } => text,
        }
    }
}

impl Serialize for Export {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Json { document, .. } => document.serialize(serializer),
            Self::Markdown { text } => {
                let mut state = serializer.serialize_struct("Export", 1)?;
                state.serialize_field("text", text)?;
                state.end()
            }
        }
    }
}

/// `tk export`: every project, archived ones included, or only `project`, with every
/// feature, task, tag and note.
///
/// Only the flag restricts the export; `TASKIST_PROJECT` and the current directory do not.
pub fn export(env: &Env, format: ExportFormat, project: Option<&str>) -> Result<Export, Error> {
    if let Some(name) = project {
        validate_name("project name", name)?;
    }
    let document = open_store(env)?.read(|tx| {
        let projects = match project {
            Some(name) => vec![project_named(tx, name)?],
            None => tx.projects(true)?,
        };
        let projects = projects
            .into_iter()
            .map(|project| export_project(tx, project))
            .collect::<Result<_, Error>>()?;
        Ok(ExportDocument {
            version: 1,
            exported_at: tx.now()?,
            projects,
        })
    })?;
    Ok(match format {
        ExportFormat::Json => {
            let mut text = serde_json::to_string_pretty(&document)
                .map_err(|err| Error::Internal(format!("cannot encode the export: {err}")))?;
            text.push('\n');
            Export::Json { document, text }
        }
        ExportFormat::Md => Export::Markdown {
            text: export_markdown(&document),
        },
    })
}

fn export_project(tx: &Tx<'_>, project: Project) -> Result<ExportProject, Error> {
    let features = tx.features(project.id)?;
    let names: HashMap<i64, &str> = features
        .iter()
        .map(|feature| (feature.id, feature.name.as_str()))
        .collect();
    let tasks = tx
        .tasks(project.id)?
        .into_iter()
        .map(|task| {
            let feature = match task.feature_id {
                None => None,
                Some(id) => Some(
                    (*names
                        .get(&id)
                        .ok_or_else(|| task::missing_feature(&task, id))?)
                    .to_owned(),
                ),
            };
            let notes = task::note_views(tx, task.id)?;
            Ok(ExportTask {
                task: task::task_view(task, &project.name, feature),
                notes,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(ExportProject {
        features: features
            .into_iter()
            .map(|feature| ExportFeature {
                name: feature.name,
                created_at: feature.created_at,
            })
            .collect(),
        tasks,
        name: project.name,
        path: project.path,
        description: project.description,
        archived: project.archived,
        created_at: project.created_at,
    })
}

/// Characters that end a line of stored text: the same set the model uses to keep titles
/// single-line (`LINE_TERMINATORS` in `model`). A carriage return followed by a line feed
/// is one break.
const LINE_TERMINATORS: [char; 7] = [
    '\n', '\u{0B}', '\u{0C}', '\r', '\u{85}', '\u{2028}', '\u{2029}',
];

/// Breaks `text` at every line terminator into line feeds and indents every line after the
/// first by `indent`, so multi-line text stays inside its list item.
fn continued(text: &str, indent: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if LINE_TERMINATORS.contains(&c) {
            if c == '\r' {
                chars.next_if_eq(&'\n');
            }
            out.push('\n');
            out.push_str(indent);
        } else {
            out.push(c);
        }
    }
    out
}

/// The markdown form of an export: a `## name` section per project with its fields, a
/// `### name` heading per feature, `### (no feature)` last, and a checkbox item per task,
/// checked when the task is closed, with its fields, body and notes as nested items.
fn export_markdown(document: &ExportDocument) -> String {
    let mut text = format!(
        "# Taskist export\n\nExported at {}.\n",
        document.exported_at
    );
    for project in &document.projects {
        let _ = write!(
            text,
            "\n## {}\n\n- path: {}\n- description: {}\n- archived: {}\n- created_at: {}\n",
            project.name,
            project.path.as_deref().unwrap_or("(none)"),
            if project.description.is_empty() {
                "(none)".to_owned()
            } else {
                continued(&project.description, "  ")
            },
            if project.archived { "yes" } else { "no" },
            project.created_at,
        );
        let mut groups: Vec<(&str, Vec<&ExportTask>)> = project
            .features
            .iter()
            .map(|feature| {
                let tasks = project
                    .tasks
                    .iter()
                    .filter(|entry| entry.task.feature.as_deref() == Some(&feature.name))
                    .collect();
                (feature.name.as_str(), tasks)
            })
            .collect();
        let loose: Vec<&ExportTask> = project
            .tasks
            .iter()
            .filter(|entry| entry.task.feature.is_none())
            .collect();
        if !loose.is_empty() {
            groups.push(("(no feature)", loose));
        }
        for (heading, tasks) in groups {
            let _ = writeln!(text, "\n### {heading}");
            if !tasks.is_empty() {
                text.push('\n');
            }
            for entry in tasks {
                task_markdown(&mut text, entry);
            }
        }
    }
    text
}

fn task_markdown(text: &mut String, entry: &ExportTask) {
    let task = &entry.task;
    let checkbox = if task.status.is_open() { "[ ]" } else { "[x]" };
    let _ = writeln!(
        text,
        "- {checkbox} #{} P{} {} {}",
        task.id, task.priority, task.status, task.title
    );
    if !task.tags.is_empty() {
        let _ = writeln!(text, "  - tags: {}", task.tags.join(", "));
    }
    let _ = writeln!(
        text,
        "  - created_at: {} by {}",
        task.created_at, task.created_by
    );
    let _ = writeln!(text, "  - updated_at: {}", task.updated_at);
    if let Some(closed_at) = &task.closed_at {
        let _ = writeln!(text, "  - closed_at: {closed_at}");
    }
    if !task.body.is_empty() {
        let _ = writeln!(text, "  - body: {}", continued(&task.body, "    "));
    }
    for note in &entry.notes {
        let _ = writeln!(
            text,
            "  - note ({}) by {} at {}: {}",
            note.kind.as_str(),
            note.author,
            note.created_at,
            continued(&note.text, "    ")
        );
    }
}

/// `import` data: `{ids}`, in file order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Imported {
    /// The ids of the created tasks, in file order.
    pub ids: Vec<i64>,
}

/// One line of an import file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportLine {
    title: String,
    project: Option<String>,
    feature: Option<String>,
    pri: Option<u8>,
    #[serde(default)]
    tags: Vec<String>,
    body: Option<String>,
    by: Option<String>,
}

/// A validated import line.
#[derive(Debug)]
struct Entry {
    line: usize,
    title: String,
    project: Option<String>,
    feature: Option<String>,
    priority: u8,
    tags: Vec<String>,
    body: String,
    actor: String,
}

/// The same error with its message prefixed by `line N: `.
fn at_line(line: usize, err: Error) -> Error {
    let prefix = |message: String| format!("line {line}: {message}");
    match err {
        Error::Internal(message) => Error::Internal(prefix(message)),
        Error::Usage(message) => Error::Usage(prefix(message)),
        Error::NotFound(message) => Error::NotFound(prefix(message)),
        Error::Conflict(message) => Error::Conflict(prefix(message)),
        Error::InvalidTransition(message) => Error::InvalidTransition(prefix(message)),
        other @ Error::UnsupportedSchema { .. } => other,
    }
}

/// The bytes of the import file, or of stdin for `-`.
fn read_source(env: &Env, file: &Path) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    if file == Path::new("-") {
        std::io::stdin()
            .lock()
            .read_to_end(&mut bytes)
            .map_err(|err| Error::Internal(format!("cannot read the import from stdin: {err}")))?;
    } else {
        let path = env.absolute(file)?;
        bytes = std::fs::read(&path).map_err(|err| {
            Error::Internal(format!(
                "cannot read the import file {}: {err}",
                path.display()
            ))
        })?;
    }
    Ok(bytes)
}

/// A parse error of one line, positioned by column only: the caller names the line of the
/// file, and the parser's own line number, always 1 within a single line, would contradict it.
fn parse_error(err: &serde_json::Error) -> Error {
    let message = err.to_string();
    if err.line() == 0 {
        return Error::Usage(message);
    }
    let position = format!(" at line {} column {}", err.line(), err.column());
    let message = message.strip_suffix(&position).unwrap_or(&message);
    Error::Usage(format!("column {}: {message}", err.column()))
}

/// Parses and validates one non-blank line.
fn entry(env: &Env, line: usize, text: &str, by: Option<&str>) -> Result<Entry, Error> {
    let parsed: ImportLine = serde_json::from_str(text).map_err(|err| parse_error(&err))?;
    let title = normalize_title(&parsed.title)?;
    let priority = validate_priority(parsed.pri.unwrap_or(DEFAULT_PRIORITY))?;
    if let Some(project) = &parsed.project {
        validate_name("project name", project)?;
    }
    if let Some(feature) = &parsed.feature {
        validate_name("feature name", feature)?;
    }
    for tag in &parsed.tags {
        validate_name("tag", tag)?;
    }
    Ok(Entry {
        line,
        title,
        project: parsed.project,
        feature: parsed.feature,
        priority,
        tags: parsed.tags,
        body: parsed.body.unwrap_or_default(),
        actor: env.actor(parsed.by.as_deref().or(by)),
    })
}

/// Parses every line; blank lines are skipped and lines are numbered from 1.
fn entries(env: &Env, bytes: &[u8], by: Option<&str>) -> Result<Vec<Entry>, Error> {
    let mut entries = Vec::new();
    for (index, raw) in bytes.split(|byte| *byte == b'\n').enumerate() {
        let line = index + 1;
        let text = std::str::from_utf8(raw)
            .map_err(|_| Error::Usage(format!("line {line}: not valid UTF-8")))?;
        if text.trim().is_empty() {
            continue;
        }
        entries.push(entry(env, line, text, by).map_err(|err| at_line(line, err))?);
    }
    Ok(entries)
}

/// `tk import`: creates one `todo` task per non-blank JSON line, all in one transaction.
///
/// Every line is parsed and validated before the store is opened. A line's `project` must
/// exist and overrides the resolved scope. A project named by `-p` or `TASKIST_PROJECT` is
/// resolved before any task is created, so an unknown one fails even when every line names
/// its own; the directory is read only for the first line without a project. Any bad line
/// fails the import, changes nothing, and the error names its line number.
pub fn import(
    env: &Env,
    file: &Path,
    request: Request<'_>,
    by: Option<&str>,
) -> Result<Imported, Error> {
    let named = request.project.is_some()
        || env
            .var("TASKIST_PROJECT")
            .is_some_and(|value| !value.is_empty());
    let entries = entries(env, &read_source(env, file)?, by)?;
    let target = scope::target(env, request)?;
    open_store(env)?.write(|tx| {
        let mut scoped = if named {
            Some(scope::resolve(tx, env, &target)?.require()?)
        } else {
            None
        };
        let ids = entries
            .iter()
            .map(|entry| {
                create_entry(tx, env, &target, &mut scoped, entry)
                    .map_err(|err| at_line(entry.line, err))
            })
            .collect::<Result<_, _>>()?;
        Ok(Imported { ids })
    })
}

/// Creates the task of one line in its own project, or in the scoped project, which is
/// resolved on first use and kept in `scoped`.
fn create_entry(
    tx: &Tx<'_>,
    env: &Env,
    target: &Target,
    scoped: &mut Option<Project>,
    entry: &Entry,
) -> Result<i64, Error> {
    let project = match (&entry.project, scoped.as_ref()) {
        (Some(name), _) => project_named(tx, name)?,
        (None, Some(project)) => project.clone(),
        (None, None) => scoped
            .insert(scope::resolve(tx, env, target)?.require()?)
            .clone(),
    };
    task::create(
        tx,
        &project,
        &Creation {
            title: &entry.title,
            feature: entry.feature.as_deref(),
            priority: entry.priority,
            tags: &entry.tags,
            body: &entry.body,
            actor: &entry.actor,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{at_line, continued, parse_error};
    use crate::error::Error;

    #[test]
    fn line_numbers_keep_the_error_kind() {
        let err = at_line(4, Error::NotFound("no project named \"x\"".into()));
        assert!(matches!(&err, Error::NotFound(_)), "{err:?}");
        assert_eq!(err.to_string(), "line 4: no project named \"x\"");
        for err in [
            Error::Internal("a".into()),
            Error::Usage("a".into()),
            Error::Conflict("a".into()),
            Error::InvalidTransition("a".into()),
        ] {
            let code = err.code();
            let numbered = at_line(1, err);
            assert_eq!(numbered.code(), code);
            assert_eq!(numbered.to_string(), "line 1: a");
        }
        let schema = at_line(
            1,
            Error::UnsupportedSchema {
                found: 2,
                supported: 1,
            },
        );
        assert_eq!(schema.code(), "unsupported_schema");
    }

    #[test]
    fn parse_errors_carry_a_column_and_no_line() {
        let err = serde_json::from_str::<serde_json::Value>("{\"a\": ").unwrap_err();
        assert_eq!(
            parse_error(&err).to_string(),
            "column 6: EOF while parsing a value"
        );
    }

    #[test]
    fn continuation_lines_are_indented() {
        assert_eq!(continued("one", "  "), "one");
        assert_eq!(continued("one\ntwo", "  "), "one\n  two");
        assert_eq!(continued("a\r\nb\rc\n\rd", "  "), "a\n  b\n  c\n  \n  d");
        for terminator in ['\u{0B}', '\u{0C}', '\u{85}', '\u{2028}', '\u{2029}'] {
            assert_eq!(continued(&format!("a{terminator}b"), "  "), "a\n  b");
        }
    }
}
