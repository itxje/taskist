//! Domain types, name and value validation, and the status transition table.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;

use crate::error::Error;

/// Longest project name, feature name or tag, in characters.
pub const MAX_NAME_LEN: usize = 64;

/// Priority given to a task when none is chosen.
pub const DEFAULT_PRIORITY: u8 = 2;

/// Least urgent priority; 0 is the most urgent.
pub const MAX_PRIORITY: u8 = 3;

/// The status of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Not started, or a follow-up.
    Todo,
    /// In progress.
    Doing,
    /// Waiting on something; the reason is a note.
    Blocked,
    /// Finished.
    Done,
    /// Will not be done; the reason is a note.
    Dropped,
}

impl Status {
    /// Every status, in the order of the data model.
    pub const ALL: [Self; 5] = [
        Self::Todo,
        Self::Doing,
        Self::Blocked,
        Self::Done,
        Self::Dropped,
    ];

    /// The name stored in the database and shown to users.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Doing => "doing",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Dropped => "dropped",
        }
    }

    /// Whether a task in this status still needs work.
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Todo | Self::Doing | Self::Blocked)
    }

    /// Whether a task in this status carries a `closed_at` time.
    pub const fn is_closed(self) -> bool {
        !self.is_open()
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Status {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Error> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
            .ok_or_else(|| {
                Error::Usage(format!(
                    "unknown status {value:?}: use todo, doing, blocked, done or dropped"
                ))
            })
    }
}

/// The kind of a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    /// A plain note.
    Note,
    /// The reason a task was blocked.
    Blocked,
    /// The note given when a task was done.
    Done,
    /// The reason a task was dropped.
    Dropped,
}

impl NoteKind {
    /// Every kind.
    pub const ALL: [Self; 4] = [Self::Note, Self::Blocked, Self::Done, Self::Dropped];

    /// The name stored in the database.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Dropped => "dropped",
        }
    }
}

impl FromStr for NoteKind {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Error> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| Error::Usage(format!("unknown note kind {value:?}")))
    }
}

/// A command that changes the status of tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// `start`
    Start,
    /// `block`
    Block,
    /// `done`
    Done,
    /// `drop`
    Drop,
    /// `reopen`
    Reopen,
}

/// What an [`Action`] does to a task in a given status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The task moves to this status.
    Change(Status),
    /// The task is already in the target status; nothing changes.
    NoOp,
    /// The table forbids the change.
    Invalid,
}

/// One row of the transition table: the target status and the statuses that change to it.
/// A task already in `to` is a no-op success; every other status is an invalid transition.
struct Rule {
    to: Status,
    from: &'static [Status],
}

impl Action {
    /// Every action.
    pub const ALL: [Self; 5] = [
        Self::Start,
        Self::Block,
        Self::Done,
        Self::Drop,
        Self::Reopen,
    ];

    /// The single transition table.
    const fn rule(self) -> Rule {
        match self {
            Self::Start => Rule {
                to: Status::Doing,
                from: &[Status::Todo, Status::Blocked],
            },
            Self::Block => Rule {
                to: Status::Blocked,
                from: &[Status::Todo, Status::Doing],
            },
            Self::Done => Rule {
                to: Status::Done,
                from: &[Status::Todo, Status::Doing, Status::Blocked],
            },
            Self::Drop => Rule {
                to: Status::Dropped,
                from: &[Status::Todo, Status::Doing, Status::Blocked],
            },
            Self::Reopen => Rule {
                to: Status::Todo,
                from: &[
                    Status::Doing,
                    Status::Blocked,
                    Status::Done,
                    Status::Dropped,
                ],
            },
        }
    }

    /// The outcome of applying this action to a task in status `from`.
    pub fn apply(self, from: Status) -> Outcome {
        let rule = self.rule();
        if rule.to == from {
            Outcome::NoOp
        } else if rule.from.contains(&from) {
            Outcome::Change(rule.to)
        } else {
            Outcome::Invalid
        }
    }
}

/// Checks a project name, feature name or tag: `[a-z0-9][a-z0-9-]*`, at most 64 characters.
///
/// `what` names the value in the error message, for example `project name`.
pub fn validate_name(what: &str, value: &str) -> Result<(), Error> {
    let mut chars = value.chars();
    let valid = value.len() <= MAX_NAME_LEN
        && chars
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if valid {
        Ok(())
    } else {
        Err(Error::Usage(format!(
            "invalid {what} {value:?}: use lowercase letters, digits and '-', \
             starting with a letter or digit, at most {MAX_NAME_LEN} characters"
        )))
    }
}

/// Trims a task title and checks that it is non-empty and a single line.
pub fn normalize_title(value: &str) -> Result<String, Error> {
    let title = value.trim();
    if title.is_empty() {
        return Err(Error::Usage("the title must not be empty".into()));
    }
    if title.contains(['\n', '\r']) {
        return Err(Error::Usage("the title must be a single line".into()));
    }
    Ok(title.to_owned())
}

/// Checks a priority: 0 (most urgent) to 3.
pub fn validate_priority(value: u8) -> Result<u8, Error> {
    if value <= MAX_PRIORITY {
        Ok(value)
    } else {
        Err(Error::Usage(format!(
            "invalid priority {value}: use 0 to {MAX_PRIORITY}"
        )))
    }
}

/// A project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Project {
    /// Database id.
    pub id: i64,
    /// Unique name.
    pub name: String,
    /// Unique directory linked to the project, if any.
    pub path: Option<String>,
    /// Free text.
    pub description: String,
    /// Whether the project is archived.
    pub archived: bool,
    /// Creation time, UTC.
    pub created_at: String,
}

/// A feature inside a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Feature {
    /// Database id.
    pub id: i64,
    /// Owning project.
    pub project_id: i64,
    /// Name, unique within the project.
    pub name: String,
    /// Creation time, UTC.
    pub created_at: String,
}

/// A task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Task {
    /// Global id, never reused.
    pub id: i64,
    /// Owning project.
    pub project_id: i64,
    /// Feature, if any.
    pub feature_id: Option<i64>,
    /// Single-line title.
    pub title: String,
    /// Free text.
    pub body: String,
    /// Current status.
    pub status: Status,
    /// 0 (most urgent) to 3.
    pub priority: u8,
    /// Tags, sorted.
    pub tags: Vec<String>,
    /// Creation time, UTC.
    pub created_at: String,
    /// Time of the last change, UTC.
    pub updated_at: String,
    /// Time the task became `done` or `dropped`.
    pub closed_at: Option<String>,
    /// Actor that created the task.
    pub created_by: String,
}

/// An append-only note on a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    /// Database id.
    pub id: i64,
    /// Task the note belongs to.
    pub task_id: i64,
    /// Kind of note.
    pub kind: NoteKind,
    /// Free text.
    pub text: String,
    /// Actor that wrote the note.
    pub author: String,
    /// Creation time, UTC.
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::{
        Action, NoteKind, Outcome, Status, normalize_title, validate_name, validate_priority,
    };
    use crate::error::Error;

    use Outcome::{Change, Invalid, NoOp};
    use Status::{Blocked, Doing, Done, Dropped, Todo};

    #[test]
    fn transition_table_matches_the_design_for_every_pair() {
        let expected = [
            (Action::Start, Todo, Change(Doing)),
            (Action::Start, Doing, NoOp),
            (Action::Start, Blocked, Change(Doing)),
            (Action::Start, Done, Invalid),
            (Action::Start, Dropped, Invalid),
            (Action::Block, Todo, Change(Blocked)),
            (Action::Block, Doing, Change(Blocked)),
            (Action::Block, Blocked, NoOp),
            (Action::Block, Done, Invalid),
            (Action::Block, Dropped, Invalid),
            (Action::Done, Todo, Change(Done)),
            (Action::Done, Doing, Change(Done)),
            (Action::Done, Blocked, Change(Done)),
            (Action::Done, Done, NoOp),
            (Action::Done, Dropped, Invalid),
            (Action::Drop, Todo, Change(Dropped)),
            (Action::Drop, Doing, Change(Dropped)),
            (Action::Drop, Blocked, Change(Dropped)),
            (Action::Drop, Done, Invalid),
            (Action::Drop, Dropped, NoOp),
            (Action::Reopen, Todo, NoOp),
            (Action::Reopen, Doing, Change(Todo)),
            (Action::Reopen, Blocked, Change(Todo)),
            (Action::Reopen, Done, Change(Todo)),
            (Action::Reopen, Dropped, Change(Todo)),
        ];
        assert_eq!(expected.len(), Action::ALL.len() * Status::ALL.len());
        for action in Action::ALL {
            for status in Status::ALL {
                assert_eq!(
                    expected
                        .iter()
                        .filter(|(a, s, _)| *a == action && *s == status)
                        .count(),
                    1,
                    "{action:?} on {status:?} listed once"
                );
            }
        }
        for (action, status, outcome) in expected {
            assert_eq!(action.apply(status), outcome, "{action:?} on {status:?}");
        }
    }

    #[test]
    fn no_rule_changes_a_status_to_itself() {
        for action in Action::ALL {
            let rule = action.rule();
            assert!(!rule.from.contains(&rule.to), "{action:?}");
        }
    }

    #[test]
    fn statuses_round_trip_and_unknown_is_a_usage_error() {
        for status in Status::ALL {
            assert_eq!(status.as_str().parse::<Status>().unwrap(), status);
            assert_eq!(status.to_string(), status.as_str());
        }
        let err = "later".parse::<Status>().unwrap_err();
        assert!(matches!(err, Error::Usage(_)), "{err:?}");
        assert_eq!(
            Status::ALL.map(Status::is_open),
            [true, true, true, false, false]
        );
        assert_eq!(
            Status::ALL.map(Status::is_closed),
            [false, false, false, true, true]
        );
    }

    #[test]
    fn note_kinds_round_trip() {
        for kind in NoteKind::ALL {
            assert_eq!(kind.as_str().parse::<NoteKind>().unwrap(), kind);
        }
        assert!(matches!("memo".parse::<NoteKind>(), Err(Error::Usage(_))));
    }

    #[test]
    fn names_accept_the_documented_pattern() {
        let longest = "a".repeat(64);
        for name in [
            "a",
            "0",
            "taskist",
            "web-ui",
            "a-",
            "9lives",
            longest.as_str(),
        ] {
            assert!(validate_name("project name", name).is_ok(), "{name:?}");
        }
    }

    #[test]
    fn names_reject_everything_else_with_a_usage_error() {
        let too_long = "a".repeat(65);
        for name in [
            "",
            "-a",
            "A",
            "web_ui",
            "web ui",
            "web.ui",
            "é",
            "a\n",
            "x;--",
            too_long.as_str(),
        ] {
            let err = validate_name("tag", name).unwrap_err();
            assert!(matches!(err, Error::Usage(_)), "{name:?}: {err:?}");
            assert!(err.to_string().starts_with("invalid tag "), "{err}");
        }
    }

    #[test]
    fn titles_are_trimmed_non_empty_and_single_line() {
        assert_eq!(normalize_title("  fix it \t").unwrap(), "fix it");
        for title in ["", "   ", "\n", "one\ntwo", "one\rtwo"] {
            let err = normalize_title(title).unwrap_err();
            assert!(matches!(err, Error::Usage(_)), "{title:?}: {err:?}");
        }
        assert_eq!(
            normalize_title("trailing newline\n").unwrap(),
            "trailing newline"
        );
    }

    #[test]
    fn priorities_are_zero_to_three() {
        for priority in 0..=3 {
            assert_eq!(validate_priority(priority).unwrap(), priority);
        }
        assert!(matches!(validate_priority(4), Err(Error::Usage(_))));
    }
}
