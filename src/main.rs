//! The `tk` binary: captures the process environment and runs the library.
#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::IsTerminal;
use std::process::ExitCode;

use anyhow::Context;
use taskist::env::Env;
use taskist::error::Error;

fn capture_env(vars: Vec<(OsString, OsString)>) -> anyhow::Result<Env> {
    let current_dir = std::env::current_dir().context("cannot read the current directory")?;
    Ok(Env::new(vars, current_dir, std::io::stdout().is_terminal()))
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let vars: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let code = match capture_env(vars) {
        Ok(env) => taskist::run(&args, &env),
        Err(err) => {
            let taskist_format = std::env::var_os("TASKIST_FORMAT");
            let error = Error::Internal(format!("{err:#}"));
            taskist::fail(&args, taskist_format.as_deref(), &error)
        }
    };
    ExitCode::from(code)
}
