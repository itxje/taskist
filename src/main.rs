//! The `tk` binary: captures the process environment and runs the library.
#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::IsTerminal;
use std::process::ExitCode;

use taskist::env::Env;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let vars: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let env =
        Env::with_current_dir_reader(vars, std::env::current_dir, std::io::stdout().is_terminal());
    ExitCode::from(taskist::run(&args, &env))
}
