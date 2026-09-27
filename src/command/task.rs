//! The task commands: `add`, `ls`, `show`, `edit`, `next` and `find`.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;

use serde::Serialize;

use super::{Tally, TaskView, listed_projects, open_store, project_named};
use crate::env::Env;
use crate::error::Error;
use crate::model::{
    DEFAULT_PRIORITY, NoteKind, Project, Status, Task, normalize_title, validate_name,
    validate_priority,
};
use crate::scope::{self, Request, Scope};
use crate::store::{NewTask, TaskUpdate, Tx};

/// `add` and `edit` data: `{task}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskData {
    /// The task after the change.
    pub task: TaskView,
}

/// A note as the output shows it: `{id, kind, text, author, created_at}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteView {
    /// Database id.
    pub id: i64,
    /// Kind of note.
    pub kind: NoteKind,
    /// Free text.
    pub text: String,
    /// Actor that wrote the note.
    pub author: String,
    /// Creation time, UTC.
    pub created_at: String,
}

/// `show` data: `{task, notes}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskShow {
    /// The task.
    pub task: TaskView,
    /// Its notes, oldest first.
    pub notes: Vec<NoteView>,
}

/// `ls` and `find` data: `{scope, tasks}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskList {
    /// The resolved scope.
    pub scope: Scope,
    /// The tasks in display order: by project name, then feature name with tasks without
    /// a feature last, then priority, then creation time, then id.
    pub tasks: Vec<TaskView>,
    /// The number of open tasks of every covered project, for the human text.
    #[serde(skip)]
    pub open: BTreeMap<String, i64>,
    /// The latest `blocked` note of every listed blocked task that has one, for the human text.
    #[serde(skip)]
    pub reasons: HashMap<i64, String>,
}

/// `next` data: `{scope, task}`, with no task when the scope has no candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NextTask {
    /// The resolved scope.
    pub scope: Scope,
    /// The task to work on next.
    pub task: Option<TaskView>,
}

/// The values `add` is given.
#[derive(Debug, Clone, Copy)]
pub struct NewTaskInput<'a> {
    /// Title, trimmed before it is stored.
    pub title: &'a str,
    /// Feature name, created when missing.
    pub feature: Option<&'a str>,
    /// Priority; [`DEFAULT_PRIORITY`] when absent.
    pub priority: Option<u8>,
    /// Tags.
    pub tags: &'a [String],
    /// Body text, or `-` for stdin.
    pub body: Option<&'a str>,
}

/// The changes `edit` asks for.
#[derive(Debug, Clone, Copy, Default)]
pub struct TaskEdit<'a> {
    /// New title.
    pub title: Option<&'a str>,
    /// New body text, or `-` for stdin.
    pub body: Option<&'a str>,
    /// New priority.
    pub priority: Option<u8>,
    /// New feature name, created when missing.
    pub feature: Option<&'a str>,
    /// Remove the task from its feature.
    pub no_feature: bool,
    /// `+TAG` or `TAG` adds a tag, `-TAG` removes one.
    pub tags: &'a [String],
    /// Target project of a move.
    pub project: Option<&'a str>,
}

/// The filters of `ls`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListFilter<'a> {
    /// Only tasks of this feature.
    pub feature: Option<&'a str>,
    /// Only tasks in these statuses; empty means the default set.
    pub statuses: &'a [Status],
    /// Only tasks with this tag.
    pub tag: Option<&'a str>,
    /// Include `done` and `dropped` tasks in the default set.
    pub all: bool,
    /// At most this many tasks, in display order.
    pub limit: Option<usize>,
}

/// Resolves a body argument: `-` reads stdin to its end, anything else is the text itself.
fn body_text(body: Option<&str>) -> Result<Option<String>, Error> {
    match body {
        Some("-") => {
            let mut text = String::new();
            std::io::stdin()
                .lock()
                .read_to_string(&mut text)
                .map_err(|err| match err.kind() {
                    std::io::ErrorKind::InvalidData => {
                        Error::Usage("the body read from stdin is not valid UTF-8".into())
                    }
                    _ => Error::Internal(format!("cannot read the body from stdin: {err}")),
                })?;
            Ok(Some(text))
        }
        other => Ok(other.map(str::to_owned)),
    }
}

fn task_view(task: Task, project: &str, feature: Option<String>) -> TaskView {
    TaskView {
        id: task.id,
        project: project.to_owned(),
        feature,
        title: task.title,
        body: task.body,
        status: task.status,
        priority: task.priority,
        tags: task.tags,
        created_at: task.created_at,
        updated_at: task.updated_at,
        closed_at: task.closed_at,
        created_by: task.created_by,
    }
}

fn missing_feature(task: &Task, feature: i64) -> Error {
    Error::Internal(format!(
        "task {} refers to the missing feature {feature}",
        task.id
    ))
}

/// The view of one task, with its project and feature names read from the store.
pub(super) fn view(tx: &Tx<'_>, task: Task) -> Result<TaskView, Error> {
    let project = tx.project_by_id(task.project_id)?.ok_or_else(|| {
        Error::Internal(format!(
            "task {} refers to the missing project {}",
            task.id, task.project_id
        ))
    })?;
    let feature = match task.feature_id {
        None => None,
        Some(id) => Some(
            tx.feature_by_id(id)?
                .ok_or_else(|| missing_feature(&task, id))?
                .name,
        ),
    };
    Ok(task_view(task, &project.name, feature))
}

pub(super) fn task_by_id(tx: &Tx<'_>, id: i64) -> Result<Task, Error> {
    tx.task(id)?
        .ok_or_else(|| Error::NotFound(format!("no task with id {id}")))
}

/// The id of the feature `name` in a project, created when it does not exist.
fn feature_id(tx: &Tx<'_>, project_id: i64, name: &str) -> Result<i64, Error> {
    match tx.feature_by_name(project_id, name)? {
        Some(feature) => Ok(feature.id),
        None => Ok(tx.insert_feature(project_id, name)?.id),
    }
}

fn refuse_archived(project: &Project) -> Result<(), Error> {
    if project.archived {
        return Err(Error::Conflict(format!(
            "project {} is archived and accepts no new tasks",
            project.name
        )));
    }
    Ok(())
}

/// `tk add`: creates a `todo` task in the resolved project, whose feature is created on
/// first use.
pub fn add(
    env: &Env,
    request: Request<'_>,
    input: NewTaskInput<'_>,
    by: Option<&str>,
) -> Result<TaskData, Error> {
    let title = normalize_title(input.title)?;
    let priority = validate_priority(input.priority.unwrap_or(DEFAULT_PRIORITY))?;
    if let Some(feature) = input.feature {
        validate_name("feature name", feature)?;
    }
    for tag in input.tags {
        validate_name("tag", tag)?;
    }
    let body = body_text(input.body)?.unwrap_or_default();
    let actor = env.actor(by);
    open_store(env)?.write(|tx| {
        let project = scope::resolve(tx, env, request)?.require()?;
        refuse_archived(&project)?;
        let feature_id = input
            .feature
            .map(|name| feature_id(tx, project.id, name))
            .transpose()?;
        let created = tx.insert_task(&NewTask {
            project_id: project.id,
            feature_id,
            title: &title,
            body: &body,
            priority,
            created_by: &actor,
        })?;
        for tag in input.tags {
            tx.add_tag(created.id, tag)?;
        }
        Ok(TaskData {
            task: view(tx, task_by_id(tx, created.id)?)?,
        })
    })
}

/// `tk show`: a task with its notes, oldest first.
pub fn show(env: &Env, id: i64) -> Result<TaskShow, Error> {
    open_store(env)?.read(|tx| {
        let task = view(tx, task_by_id(tx, id)?)?;
        let notes = tx
            .notes(id)?
            .into_iter()
            .map(|note| NoteView {
                id: note.id,
                kind: note.kind,
                text: note.text,
                author: note.author,
                created_at: note.created_at,
            })
            .collect();
        Ok(TaskShow { task, notes })
    })
}

/// One tag change of `edit`: whether it adds, and the tag.
fn tag_change(value: &str) -> Result<(bool, &str), Error> {
    let (adds, tag) = value.strip_prefix('-').map_or_else(
        || (true, value.strip_prefix('+').unwrap_or(value)),
        |tag| (false, tag),
    );
    validate_name("tag", tag)?;
    Ok((adds, tag))
}

/// `tk edit`: changes the given fields of a task; `-p` moves it to another project, where
/// its feature is matched by name and created when missing.
pub fn edit(env: &Env, id: i64, change: TaskEdit<'_>) -> Result<TaskData, Error> {
    if change.title.is_none()
        && change.body.is_none()
        && change.priority.is_none()
        && change.feature.is_none()
        && !change.no_feature
        && change.tags.is_empty()
        && change.project.is_none()
    {
        return Err(Error::Usage(
            "nothing to change: pass --title, --body, --pri, -f/--feature, --no-feature, \
             --tag or -p/--project"
                .into(),
        ));
    }
    let title = change.title.map(normalize_title).transpose()?;
    let priority = change.priority.map(validate_priority).transpose()?;
    if let Some(feature) = change.feature {
        validate_name("feature name", feature)?;
    }
    let tags = change
        .tags
        .iter()
        .map(|tag| tag_change(tag))
        .collect::<Result<Vec<_>, _>>()?;
    let body = body_text(change.body)?;
    open_store(env)?.write(|tx| {
        let task = task_by_id(tx, id)?;
        let target = match change.project {
            Some(name) => project_named(tx, name)?,
            None => tx.project_by_id(task.project_id)?.ok_or_else(|| {
                Error::Internal(format!(
                    "task {id} refers to the missing project {}",
                    task.project_id
                ))
            })?,
        };
        let moving = target.id != task.project_id;
        if moving {
            refuse_archived(&target)?;
        }
        let feature_id = if change.no_feature {
            None
        } else if let Some(name) = change.feature {
            Some(feature_id(tx, target.id, name)?)
        } else if let (true, Some(current)) = (moving, task.feature_id) {
            let name = tx
                .feature_by_id(current)?
                .ok_or_else(|| missing_feature(&task, current))?
                .name;
            Some(feature_id(tx, target.id, &name)?)
        } else {
            task.feature_id
        };
        tx.update_task(
            id,
            &TaskUpdate {
                project_id: target.id,
                feature_id,
                title: title.as_deref().unwrap_or(&task.title),
                body: body.as_deref().unwrap_or(&task.body),
                priority: priority.unwrap_or(task.priority),
            },
        )?;
        for (adds, tag) in tags {
            if adds {
                tx.add_tag(id, tag)?;
            } else {
                tx.remove_tag(id, tag)?;
            }
        }
        Ok(TaskData {
            task: view(tx, task_by_id(tx, id)?)?,
        })
    })
}

/// A project a listing covers, with the names of its features by id.
struct Covered {
    project: Project,
    features: HashMap<i64, String>,
}

fn covered(tx: &Tx<'_>, scope: &Scope) -> Result<Vec<Covered>, Error> {
    listed_projects(tx, scope)?
        .into_iter()
        .map(|project| {
            let features = tx
                .features(project.id)?
                .into_iter()
                .map(|feature| (feature.id, feature.name))
                .collect();
            Ok(Covered { project, features })
        })
        .collect()
}

/// Refuses a feature filter that names no feature of the covered projects.
fn check_feature(scope: &Scope, covered: &[Covered], feature: Option<&str>) -> Result<(), Error> {
    let Some(name) = feature else {
        return Ok(());
    };
    if covered
        .iter()
        .any(|entry| entry.features.values().any(|known| known == name))
    {
        return Ok(());
    }
    Err(Error::NotFound(scope.project.as_ref().map_or_else(
        || format!("no feature named {name:?} in any listed project"),
        |project| format!("no feature named {name:?} in project {}", project.name),
    )))
}

/// The tasks of the covered projects that `keep` accepts, in no particular order.
fn collect(
    tx: &Tx<'_>,
    covered: &[Covered],
    mut keep: impl FnMut(&TaskView) -> Result<bool, Error>,
) -> Result<Vec<TaskView>, Error> {
    let mut tasks = Vec::new();
    for entry in covered {
        for task in tx.tasks(entry.project.id)? {
            let feature = match task.feature_id {
                None => None,
                Some(id) => Some(
                    entry
                        .features
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| missing_feature(&task, id))?,
                ),
            };
            let view = task_view(task, &entry.project.name, feature);
            if keep(&view)? {
                tasks.push(view);
            }
        }
    }
    Ok(tasks)
}

/// The display order of listings.
fn display_key(task: &TaskView) -> (&str, bool, Option<&str>, u8, &str, i64) {
    (
        &task.project,
        task.feature.is_none(),
        task.feature.as_deref(),
        task.priority,
        &task.created_at,
        task.id,
    )
}

/// Sorts `tasks` into display order, applies `limit` and gathers what the human text needs.
fn task_list(
    tx: &Tx<'_>,
    scope: Scope,
    covered: &[Covered],
    mut tasks: Vec<TaskView>,
    limit: Option<usize>,
) -> Result<TaskList, Error> {
    tasks.sort_by(|a, b| display_key(a).cmp(&display_key(b)));
    if let Some(limit) = limit {
        tasks.truncate(limit);
    }
    let open = covered
        .iter()
        .map(|entry| {
            Ok((
                entry.project.name.clone(),
                Tally::of(tx, entry.project.id)?.open(),
            ))
        })
        .collect::<Result<_, Error>>()?;
    let mut reasons = HashMap::new();
    for task in tasks.iter().filter(|task| task.status == Status::Blocked) {
        let latest = tx
            .notes(task.id)?
            .into_iter()
            .rev()
            .find(|note| note.kind == NoteKind::Blocked);
        if let Some(note) = latest {
            reasons.insert(task.id, note.text);
        }
    }
    Ok(TaskList {
        scope,
        tasks,
        open,
        reasons,
    })
}

/// `tk ls`: the tasks of the resolved scope that pass the filters, in display order.
pub fn list(env: &Env, request: Request<'_>, filter: ListFilter<'_>) -> Result<TaskList, Error> {
    if let Some(feature) = filter.feature {
        validate_name("feature name", feature)?;
    }
    if let Some(tag) = filter.tag {
        validate_name("tag", tag)?;
    }
    open_store(env)?.read(|tx| {
        let scope = scope::resolve(tx, env, request)?;
        let covered = covered(tx, &scope)?;
        check_feature(&scope, &covered, filter.feature)?;
        let tasks = collect(tx, &covered, |task| {
            let status = if filter.statuses.is_empty() {
                filter.all || task.status.is_open()
            } else {
                filter.statuses.contains(&task.status)
            };
            let feature = filter
                .feature
                .is_none_or(|name| task.feature.as_deref() == Some(name));
            let tag = filter
                .tag
                .is_none_or(|tag| task.tags.iter().any(|own| own == tag));
            Ok(status && feature && tag)
        })?;
        task_list(tx, scope, &covered, tasks, filter.limit)
    })
}

/// `tk next`: the first `todo` or `doing` task by priority, then `doing` before `todo`,
/// then creation time, then id.
pub fn next(env: &Env, request: Request<'_>, feature: Option<&str>) -> Result<NextTask, Error> {
    if let Some(feature) = feature {
        validate_name("feature name", feature)?;
    }
    open_store(env)?.read(|tx| {
        let scope = scope::resolve(tx, env, request)?;
        let covered = covered(tx, &scope)?;
        check_feature(&scope, &covered, feature)?;
        let candidates = collect(tx, &covered, |task| {
            Ok(matches!(task.status, Status::Todo | Status::Doing)
                && feature.is_none_or(|name| task.feature.as_deref() == Some(name)))
        })?;
        let key = |task: &TaskView| {
            (
                task.priority,
                task.status != Status::Doing,
                task.created_at.clone(),
                task.id,
            )
        };
        let task = candidates.into_iter().min_by_key(key);
        Ok(NextTask { scope, task })
    })
}

/// Folds case without context: every character is replaced by the lowercase of its
/// uppercase, repeated until the text no longer changes.
///
/// The result is stable, so folding folded text changes nothing, and a character and each
/// of its lowercase and uppercase forms fold to the same text; the unit tests check both
/// over every Unicode scalar value. A single pass is not enough: the capital sharp s `ẞ`
/// is its own uppercase and folds to `ß` in one pass, whose uppercase `SS` folds to `ss`.
///
/// `str::to_lowercase` is not used because it lowercases a capital sigma by its position in
/// the word, which would fold the same letter differently in a query and in a text.
fn fold_case(text: &str) -> String {
    let pass = |text: &str| -> String {
        text.chars()
            .flat_map(char::to_uppercase)
            .flat_map(char::to_lowercase)
            .collect()
    };
    let mut folded = pass(text);
    loop {
        let next = pass(&folded);
        if next == folded {
            return folded;
        }
        folded = next;
    }
}

/// `tk find`: the tasks of the resolved scope whose title, body or note text contains
/// `query`, ignoring case; open tasks only unless `all`.
///
/// The comparison is a plain substring test on case-folded text, so `%`, `_` and `\` in
/// the query match only themselves.
pub fn find(env: &Env, request: Request<'_>, query: &str, all: bool) -> Result<TaskList, Error> {
    let needle = fold_case(query);
    let matches = |text: &str| fold_case(text).contains(&needle);
    open_store(env)?.read(|tx| {
        let scope = scope::resolve(tx, env, request)?;
        let covered = covered(tx, &scope)?;
        let tasks = collect(tx, &covered, |task| {
            if !(all || task.status.is_open()) {
                return Ok(false);
            }
            if matches(&task.title) || matches(&task.body) {
                return Ok(true);
            }
            Ok(tx.notes(task.id)?.iter().any(|note| matches(&note.text)))
        })?;
        task_list(tx, scope, &covered, tasks, None)
    })
}

#[cfg(test)]
mod tests {
    use super::{fold_case, tag_change};
    use crate::error::Error;

    #[test]
    fn case_folding_does_not_depend_on_the_position_in_a_word() {
        // Calibration: plain ASCII folds to lowercase.
        assert_eq!(fold_case("Token"), "token");
        assert_eq!(fold_case("ΟΔΟΣ"), "οδοσ");
        for sigma in ["Σ", "σ", "ς"] {
            assert_eq!(fold_case(sigma), "σ", "{sigma}");
        }
        assert_eq!(fold_case("Straße"), fold_case("STRASSE"));
        assert_eq!(fold_case("\u{212a}"), "k");
        // The capital sharp s folds like its lowercase and like SS.
        for sharp_s in ["ẞ", "ß", "SS", "ss"] {
            assert_eq!(fold_case(sharp_s), "ss", "{sharp_s}");
        }
        assert_eq!(fold_case("GROẞ"), fold_case("groß"));
        assert_eq!(fold_case("STRAẞE"), fold_case("straße"));
    }

    #[test]
    fn every_character_folds_like_its_case_variants_and_folding_is_stable() {
        let mut checked = 0_u32;
        for c in (0..=0x0010_ffff_u32).filter_map(char::from_u32) {
            let own = c.to_string();
            let folded = fold_case(&own);
            assert_eq!(fold_case(&folded), folded, "{c:?} U+{:04X}", u32::from(c));
            for variant in [c.to_lowercase().to_string(), c.to_uppercase().to_string()] {
                assert_eq!(
                    fold_case(&variant),
                    folded,
                    "{c:?} U+{:04X} and {variant:?}",
                    u32::from(c)
                );
            }
            checked += 1;
        }
        // Calibration: every Unicode scalar value was visited.
        assert_eq!(checked, 0x0011_0000 - 0x800);
    }

    #[test]
    fn tag_changes_add_by_default_and_remove_with_a_dash() {
        assert_eq!(tag_change("ui").unwrap(), (true, "ui"));
        assert_eq!(tag_change("+ui").unwrap(), (true, "ui"));
        assert_eq!(tag_change("-ui").unwrap(), (false, "ui"));
        for bad in ["", "+", "-", "--ui", "+-ui", "UI", "-a_b"] {
            assert!(matches!(tag_change(bad), Err(Error::Usage(_))), "{bad:?}");
        }
    }
}
