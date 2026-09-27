//! Command-line definitions.

use clap::{Parser, Subcommand};

/// Track unfinished and follow-up work across many projects.
#[derive(Debug, Parser)]
#[command(name = "tk", version, arg_required_else_help = false)]
pub struct Cli {
    /// Print one JSON envelope instead of human text.
    #[arg(long, global = true)]
    pub json: bool,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The commands `tk` understands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Manage projects.
    Project {
        /// The project command to run.
        #[command(subcommand)]
        command: ProjectCommand,
    },
}

/// The `tk project` commands.
#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// List projects.
    Ls,
}
