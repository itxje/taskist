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
pub mod store;

use std::ffi::{OsStr, OsString};

use clap::Parser;
use clap::error::ErrorKind;

use crate::cli::{Cli, Command, ProjectCommand};
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
        Command::Project {
            command: ProjectCommand::Ls,
        } => match command::project_list(env) {
            Ok(projects) => output::emit_projects(format, &projects),
            Err(err) => output::emit_failure(format, &err),
        },
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
