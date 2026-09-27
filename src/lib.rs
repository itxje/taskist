//! taskist: track unfinished and follow-up work across many projects.
//!
//! The `tk` binary captures the process environment into an [`env::Env`] and calls [`run`].
#![forbid(unsafe_code)]

pub mod cli;
pub mod command;
pub mod env;
pub mod error;
pub mod model;
pub mod output;
pub mod scope;
pub mod store;

use std::ffi::{OsStr, OsString};

use clap::Parser;
use clap::error::ErrorKind;

use crate::cli::{Cli, Command, FeatureCommand, ProjectCommand};
use crate::command::project::ProjectEdit;
use crate::env::Env;
use crate::error::Error;
use crate::output::Format;

/// Runs one invocation of `tk` and returns the process exit code.
///
/// `args` are the raw process arguments, program name first.
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
    match cli.command {
        Command::Ls { scope } => output::emit(
            format,
            command::list(env, scope.request()),
            output::task_list_text,
        ),
        Command::Project { command } => run_project(format, env, command),
        Command::Feature { command } => run_feature(format, env, command),
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
pub fn fail(args: &[OsString], taskist_format: Option<&OsStr>, error: &Error) -> u8 {
    let json_flag = args.iter().skip(1).any(|arg| arg == "--json");
    match Format::detect(json_flag, taskist_format) {
        Ok(format) => output::emit_failure(format, error),
        Err(format_error) => output::emit_failure(Format::Text, &format_error),
    }
}
