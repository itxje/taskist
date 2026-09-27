//! Command-line definitions.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::scope::Request;

/// Track unfinished and follow-up work across many projects.
#[derive(Debug, Parser)]
#[command(name = "tk", version, arg_required_else_help = false)]
pub struct Cli {
    /// Print one JSON envelope instead of human text.
    #[arg(long, global = true)]
    pub json: bool,

    /// The actor recorded on created tasks and notes [default: `TASKIST_ACTOR`, `USER`, unknown].
    #[arg(long, global = true, value_name = "ACTOR")]
    pub by: Option<String>,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The commands `tk` understands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// List the open tasks of the resolved scope.
    Ls {
        /// Scope options.
        #[command(flatten)]
        scope: ListScope,
    },
    /// Manage projects.
    Project {
        /// The project command to run.
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// Manage features.
    Feature {
        /// The feature command to run.
        #[command(subcommand)]
        command: FeatureCommand,
    },
}

/// Scope options of commands that write to one project.
#[derive(Debug, Args)]
pub struct ProjectScope {
    /// The project [default: `TASKIST_PROJECT`, then the current directory].
    #[arg(short = 'p', long = "project", value_name = "NAME")]
    pub project: Option<String>,
}

impl ProjectScope {
    /// The scope request these options make.
    pub fn request(&self) -> Request<'_> {
        Request {
            project: self.project.as_deref(),
            all_projects: false,
        }
    }
}

/// Scope options of listing commands.
#[derive(Debug, Args)]
pub struct ListScope {
    /// The project [default: `TASKIST_PROJECT`, then the current directory, then all projects].
    #[arg(short = 'p', long = "project", value_name = "NAME")]
    pub project: Option<String>,

    /// Cover every non-archived project, ignoring `TASKIST_PROJECT` and the current directory.
    #[arg(long)]
    pub all_projects: bool,
}

impl ListScope {
    /// The scope request these options make.
    pub fn request(&self) -> Request<'_> {
        Request {
            project: self.project.as_deref(),
            all_projects: self.all_projects,
        }
    }
}

/// The `tk project` commands.
#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// Register a project.
    Add {
        /// Project name: lowercase letters, digits and '-'.
        name: String,
        /// Directory linked to the project; commands run inside it target the project.
        #[arg(long, value_name = "DIR")]
        path: Option<PathBuf>,
        /// Description.
        #[arg(long, value_name = "TEXT")]
        desc: Option<String>,
    },
    /// List projects with their open-task counts.
    Ls {
        /// Include archived projects.
        #[arg(long)]
        all: bool,
    },
    /// Show a project, its features and its task counts per status.
    Show {
        /// Project name.
        name: String,
    },
    /// Change a project; at least one option is required.
    Edit {
        /// Project name.
        name: String,
        /// New name.
        #[arg(long = "name", value_name = "NAME")]
        new_name: Option<String>,
        /// New linked directory.
        #[arg(long, value_name = "DIR", conflicts_with = "no_path")]
        path: Option<PathBuf>,
        /// Remove the linked directory.
        #[arg(long)]
        no_path: bool,
        /// New description.
        #[arg(long, value_name = "TEXT")]
        desc: Option<String>,
    },
    /// Archive a project, or unarchive it with --undo.
    Archive {
        /// Project name.
        name: String,
        /// Unarchive instead.
        #[arg(long)]
        undo: bool,
    },
    /// Delete a project; one that has tasks needs --force.
    Rm {
        /// Project name.
        name: String,
        /// Also delete its features, tasks, tags and notes.
        #[arg(long)]
        force: bool,
    },
}

/// The `tk feature` commands.
#[derive(Debug, Subcommand)]
pub enum FeatureCommand {
    /// List features with their open and total task counts.
    Ls {
        /// Scope options.
        #[command(flatten)]
        scope: ListScope,
    },
    /// Rename a feature, or merge it into an existing one.
    Mv {
        /// Current feature name.
        old: String,
        /// New feature name; when it exists, the tasks of <OLD> move there.
        new: String,
        /// Scope options.
        #[command(flatten)]
        scope: ProjectScope,
    },
}
