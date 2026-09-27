//! The status commands `start`, `block`, `done`, `drop` and `reopen`, and `note`.

use serde::Serialize;

use super::task::{TaskData, task_by_id, view};
use super::{TaskView, open_store};
use crate::env::Env;
use crate::error::Error;
use crate::model::{Action, NoteKind, Outcome};

/// The status commands' data: `{tasks}`, in argument order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransitionData {
    /// One entry per id argument.
    pub tasks: Vec<Transition>,
}

/// One task of a status command: `{task, changed}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Transition {
    /// The task after the command.
    pub task: TaskView,
    /// Whether its status changed; false for a no-op.
    pub changed: bool,
}

/// Splits the arguments of `done`: the leading ones that are integers are ids and at most
/// one following argument is the note; `flag` is the note given with `--note`.
///
/// An integer here is a non-empty run of ASCII digits, so a note such as `+1` or `-1` is
/// text, not an id; one too large for an id is a `usage` error rather than a note.
pub fn split_done_args(
    args: &[String],
    flag: Option<String>,
) -> Result<(Vec<i64>, Option<String>), Error> {
    let mut ids = Vec::new();
    for arg in args {
        if arg.is_empty() || !arg.bytes().all(|byte| byte.is_ascii_digit()) {
            break;
        }
        ids.push(
            arg.parse()
                .map_err(|_| Error::Usage(format!("task id {arg} is too large")))?,
        );
    }
    if ids.is_empty() {
        return Err(Error::Usage(
            "done needs at least one task id before the note".into(),
        ));
    }
    let rest = &args[ids.len()..];
    match (rest, flag) {
        ([], flag) => Ok((ids, flag)),
        ([note], None) => Ok((ids, Some(note.clone()))),
        ([_], Some(_)) => Err(Error::Usage(
            "give the note either as an argument or with --note, not both".into(),
        )),
        ([_, extra, ..], _) => Err(Error::Usage(format!(
            "unexpected argument {extra:?} after the note: ids come first and the note is \
             one argument; quote it or pass --note"
        ))),
    }
}

/// Applies `action` to every task in `ids`, in order, in one transaction.
///
/// An unknown id or a transition the table forbids fails the command and changes nothing.
/// A task already in the target status is left untouched. A task that changes gets `note`,
/// when given, written by the resolved actor.
pub fn transition(
    env: &Env,
    action: Action,
    ids: &[i64],
    note: Option<(NoteKind, &str)>,
    by: Option<&str>,
) -> Result<TransitionData, Error> {
    let actor = env.actor(by);
    open_store(env)?.write(|tx| {
        let mut tasks = Vec::with_capacity(ids.len());
        for &id in ids {
            let task = task_by_id(tx, id)?;
            let changed = match action.apply(task.status) {
                Outcome::NoOp => false,
                Outcome::Invalid => {
                    return Err(Error::InvalidTransition(format!(
                        "task #{id} is {} and cannot become {}",
                        task.status,
                        action.target()
                    )));
                }
                Outcome::Change(status) => {
                    tx.set_status(id, status)?;
                    if let Some((kind, text)) = note {
                        tx.insert_note(id, kind, text, &actor)?;
                    }
                    true
                }
            };
            let task = if changed { task_by_id(tx, id)? } else { task };
            tasks.push(Transition {
                task: view(tx, task)?,
                changed,
            });
        }
        Ok(TransitionData { tasks })
    })
}

/// `tk note`: appends a note of kind `note` to a task and moves its update time.
pub fn note(env: &Env, id: i64, text: &str, by: Option<&str>) -> Result<TaskData, Error> {
    let actor = env.actor(by);
    open_store(env)?.write(|tx| {
        task_by_id(tx, id)?;
        tx.insert_note(id, NoteKind::Note, text, &actor)?;
        tx.touch_task(id)?;
        Ok(TaskData {
            task: view(tx, task_by_id(tx, id)?)?,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::split_done_args;
    use crate::error::Error;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn done_arguments_are_ids_then_one_note() {
        assert_eq!(
            split_done_args(&args(&["1"]), None).unwrap(),
            (vec![1], None)
        );
        assert_eq!(
            split_done_args(&args(&["1", "2", "text"]), None).unwrap(),
            (vec![1, 2], Some("text".into()))
        );
        assert_eq!(
            split_done_args(&args(&["1", "2", "3"]), None).unwrap(),
            (vec![1, 2, 3], None)
        );
        assert_eq!(
            split_done_args(&args(&["1"]), Some("42".into())).unwrap(),
            (vec![1], Some("42".into()))
        );
    }

    #[test]
    fn done_arguments_without_ids_or_with_two_notes_are_usage_errors() {
        for (values, flag) in [
            (&["text"][..], None),
            (&[][..], Some("text")),
            (&["1", "text", "more"][..], None),
            (&["1", "text", "2"][..], None),
            (&["1", "text"][..], Some("other")),
        ] {
            let result = split_done_args(&args(values), flag.map(str::to_owned));
            assert!(
                matches!(result, Err(Error::Usage(_))),
                "{values:?}: {result:?}"
            );
        }
    }

    #[test]
    fn only_plain_digits_are_ids() {
        // Calibration: a plain number is an id.
        assert_eq!(
            split_done_args(&args(&["2", "7"]), None).unwrap(),
            (vec![2, 7], None)
        );
        for note in ["+1", "-1", " 1", "1 ", "1e3", "0x1", "١"] {
            assert_eq!(
                split_done_args(&args(&["2", note]), None).unwrap(),
                (vec![2], Some(note.to_owned())),
                "{note:?}"
            );
        }
        assert_eq!(
            split_done_args(&args(&["007"]), None).unwrap(),
            (vec![7], None)
        );
        let largest = i64::MAX.to_string();
        assert_eq!(
            split_done_args(&args(&[&largest]), None).unwrap(),
            (vec![i64::MAX], None)
        );
    }

    #[test]
    fn a_number_too_large_for_an_id_is_a_usage_error_naming_it() {
        for value in ["9223372036854775808", "99999999999999999999"] {
            for values in [&["2", value][..], &[value][..]] {
                let err = split_done_args(&args(values), None).unwrap_err();
                assert!(matches!(err, Error::Usage(_)), "{values:?}: {err:?}");
                assert!(err.to_string().contains(value), "{err}");
            }
        }
    }
}
