//! taskist: track unfinished and follow-up work across many projects.
//!
//! The `tk` binary captures the process environment into an [`env::Env`] and calls [`run`].
#![forbid(unsafe_code)]

pub mod cli;
pub mod command;
pub mod env;
pub mod error;
pub mod guide;
pub mod model;
pub mod output;
pub mod scope;
pub mod store;

use std::ffi::{OsStr, OsString};

use clap::Parser;
use clap::error::ErrorKind;

use crate::cli::{Cli, Command, ExchangeCommand, FeatureCommand, ProjectCommand, StatusCommand};
use crate::command::project::ProjectEdit;
use crate::command::task::{ListFilter, NewTaskInput, TaskEdit};
use crate::env::Env;
use crate::error::Error;
use crate::model::{Action, NoteKind};
use crate::output::{Format, Paint};

/// Runs one invocation of `tk` and returns the process exit code.
///
/// `args` are the raw process arguments, program name first.
///
/// ```
/// use std::ffi::OsString;
/// use taskist::env::Env;
///
/// let dir = tempfile::tempdir()?;
/// let env = Env::new(
///     [(OsString::from("TASKIST_DB"), dir.path().join("tk.db").into_os_string())],
///     dir.path().to_path_buf(),
///     false,
/// );
/// let tk = |args: &[&str]| {
///     let args: Vec<OsString> = ["tk"].iter().chain(args).map(OsString::from).collect();
///     taskist::run(&args, &env)
/// };
/// assert_eq!(tk(&["project", "add", "web"]), 0);
/// assert_eq!(tk(&["add", "Fix login", "-p", "web"]), 0);
/// assert_eq!(tk(&["done", "1"]), 0);
/// assert_eq!(tk(&["start", "1"]), 4); // invalid_transition
/// assert_eq!(tk(&["show", "7"]), 3); // not_found
/// assert_eq!(tk(&["frobnicate"]), 2); // usage
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn run(args: &[OsString], env: &Env) -> u8 {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err) => {
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                return output::emit_text(&err.to_string());
            }
            return fail(args, env.var("TASKIST_FORMAT"), &Error::from(err));
        }
    };
    let format = match Format::select(cli.json, env) {
        Ok(format) => format,
        Err(err) => return output::emit_failure(Format::Text, &err),
    };
    run_command(format, env, cli.by.as_deref(), cli.command)
}

fn run_command(format: Format, env: &Env, by: Option<&str>, command: Command) -> u8 {
    use command::task;
    let paint = Paint::new(env.colour());
    match command {
        Command::Add {
            title,
            scope,
            feature,
            priority,
            tags,
            body,
        } => output::emit(
            format,
            task::add(
                env,
                scope.request(),
                NewTaskInput {
                    title: &title,
                    feature: feature.as_deref(),
                    priority,
                    tags: &tags,
                    body: body.as_deref(),
                },
                by,
            ),
            output::task_added_text,
        ),
        Command::Ls {
            scope,
            feature,
            status,
            tag,
            all,
            limit,
        } => output::emit(
            format,
            task::list(
                env,
                scope.request(),
                ListFilter {
                    feature: feature.as_deref(),
                    statuses: &status,
                    tag: tag.as_deref(),
                    all,
                    limit,
                },
            ),
            |list| output::task_list_text(list, paint),
        ),
        Command::Show { id } => output::emit(format, task::show(env, id), |show| {
            output::task_show_text(show, paint)
        }),
        Command::Edit {
            id,
            title,
            body,
            priority,
            feature,
            no_feature,
            tags,
            project,
        } => output::emit(
            format,
            task::edit(
                env,
                id,
                TaskEdit {
                    title: title.as_deref(),
                    body: body.as_deref(),
                    priority,
                    feature: feature.as_deref(),
                    no_feature,
                    tags: &tags,
                    project: project.as_deref(),
                },
            ),
            output::task_updated_text,
        ),
        Command::Status(command) => run_status(format, env, by, command),
        Command::Next { scope, feature } => output::emit(
            format,
            task::next(env, scope.request(), feature.as_deref()),
            |next| output::next_task_text(next, paint),
        ),
        Command::Find { query, scope, all } => output::emit(
            format,
            task::find(env, scope.request(), &query, all),
            |list| output::task_list_text(list, paint),
        ),
        Command::Exchange(command) => run_exchange(format, env, by, command),
        Command::Project { command } => run_project(format, env, command),
        Command::Feature { command } => run_feature(format, env, command),
        Command::Guide => output::emit(format, Ok(guide::guide()), output::document_text),
        Command::Completions { shell } => {
            output::emit(format, guide::completions(shell), output::document_text)
        }
    }
}

fn run_status(format: Format, env: &Env, by: Option<&str>, command: StatusCommand) -> u8 {
    match command {
        StatusCommand::Start { ids } => transition(format, env, Action::Start, &ids, None, by),
        StatusCommand::Block { id, reason } => transition(
            format,
            env,
            Action::Block,
            &[id],
            Some((NoteKind::Blocked, &reason)),
            by,
        ),
        StatusCommand::Done { args, note } => {
            match command::transition::split_done_args(&args, note) {
                Ok((ids, note)) => transition(
                    format,
                    env,
                    Action::Done,
                    &ids,
                    note.as_deref().map(|text| (NoteKind::Done, text)),
                    by,
                ),
                Err(err) => output::emit_failure(format, &err),
            }
        }
        StatusCommand::Drop { id, reason } => transition(
            format,
            env,
            Action::Drop,
            &[id],
            Some((NoteKind::Dropped, &reason)),
            by,
        ),
        StatusCommand::Reopen { ids } => transition(format, env, Action::Reopen, &ids, None, by),
        StatusCommand::Note { id, text } => output::emit(
            format,
            command::transition::note(env, id, &text, by),
            output::task_noted_text,
        ),
    }
}

fn transition(
    format: Format,
    env: &Env,
    action: Action,
    ids: &[i64],
    note: Option<(NoteKind, &str)>,
    by: Option<&str>,
) -> u8 {
    output::emit(
        format,
        command::transition::transition(env, action, ids, note, by),
        output::transition_text,
    )
}

fn run_exchange(format: Format, env: &Env, by: Option<&str>, command: ExchangeCommand) -> u8 {
    use command::exchange;
    match command {
        ExchangeCommand::Brief { scope } => output::emit(
            format,
            exchange::brief(env, scope.request()),
            output::brief_text,
        ),
        ExchangeCommand::Export {
            format: document,
            project,
        } => output::emit(
            format,
            exchange::export(env, document, project.as_deref()),
            output::export_text,
        ),
        ExchangeCommand::Import { file, scope } => output::emit(
            format,
            exchange::import(env, &file, scope.request(), by),
            output::import_text,
        ),
    }
}

fn run_project(format: Format, env: &Env, command: ProjectCommand) -> u8 {
    use command::project;
    match command {
        ProjectCommand::Add { name, path, desc } => output::emit(
            format,
            project::add(env, &name, path.as_deref(), desc.as_deref()),
            output::project_added_text,
        ),
        ProjectCommand::Ls { all } => {
            output::emit(format, project::list(env, all), output::project_list_text)
        }
        ProjectCommand::Show { name } => {
            output::emit(format, project::show(env, &name), output::project_show_text)
        }
        ProjectCommand::Edit {
            name,
            new_name,
            path,
            no_path,
            desc,
        } => output::emit(
            format,
            project::edit(
                env,
                &name,
                ProjectEdit {
                    name: new_name.as_deref(),
                    path: path.as_deref(),
                    no_path,
                    description: desc.as_deref(),
                },
            ),
            output::project_updated_text,
        ),
        ProjectCommand::Archive { name, undo } => output::emit(
            format,
            project::archive(env, &name, undo),
            output::project_archive_text,
        ),
        ProjectCommand::Rm { name, force } => output::emit(
            format,
            project::remove(env, &name, force),
            output::project_removal_text,
        ),
    }
}

fn run_feature(format: Format, env: &Env, command: FeatureCommand) -> u8 {
    use command::feature;
    match command {
        FeatureCommand::Ls { scope } => output::emit(
            format,
            feature::list(env, scope.request()),
            output::feature_list_text,
        ),
        FeatureCommand::Mv { old, new, scope } => output::emit(
            format,
            feature::rename(env, &old, &new, scope.request()),
            output::feature_move_text,
        ),
    }
}

/// Reports a failure found before a command ran, such as an argument parsing failure
/// or an environment that could not be captured, and returns its exit code.
///
/// The error is JSON when `--json` appears among the raw arguments or
/// `TASKIST_FORMAT` is `json`. An invalid `TASKIST_FORMAT` is reported instead,
/// as text, since the requested format is unknown.
///
/// ```
/// use std::ffi::{OsStr, OsString};
/// use taskist::error::Error;
///
/// let args = [OsString::from("tk"), OsString::from("--json")];
/// let error = Error::NotFound("no task 7".into());
/// assert_eq!(taskist::fail(&args, None, &error), 3);
/// let usage = Error::Usage("bad".into());
/// assert_eq!(taskist::fail(&args[..1], Some(OsStr::new("yaml")), &usage), 2);
/// ```
pub fn fail(args: &[OsString], taskist_format: Option<&OsStr>, error: &Error) -> u8 {
    let json_flag = args.iter().skip(1).any(|arg| arg == "--json");
    match Format::detect(json_flag, taskist_format) {
        Ok(format) => output::emit_failure(format, error),
        Err(format_error) => output::emit_failure(Format::Text, &format_error),
    }
}
