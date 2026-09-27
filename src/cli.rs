//! Command-line definitions.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::model::Status;
use crate::scope::Request;

/// Track unfinished and follow-up work across many projects.
#[derive(Debug, Parser)]
#[command(name = "tk", version, arg_required_else_help = false)]
pub struct Cli {
    /// Print one JSON envelope instead of human text.
    #[arg(long, global = true)]
    pub json: bool,

    /// The actor recorded on created tasks and notes [default: `TASKIST_ACTOR`, `USER`, unknown].
    #[arg(long, global = true, value_name = "ACTOR", allow_hyphen_values = true)]
    pub by: Option<String>,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The commands `tk` understands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a task; its feature is created when it does not exist.
    Add {
        /// Single-line title.
        #[arg(value_name = "TITLE")]
        title: String,
        /// Scope options.
        #[command(flatten)]
        scope: ProjectScope,
        /// Feature name.
        #[arg(
            short = 'f',
            long = "feature",
            value_name = "NAME",
            allow_hyphen_values = true
        )]
        feature: Option<String>,
        /// Priority, 0 (most urgent) to 3 [default: 2].
        #[arg(long = "pri", value_name = "N", allow_hyphen_values = true)]
        priority: Option<u8>,
        /// Tag; repeat for several.
        #[arg(long = "tag", value_name = "TAG", allow_hyphen_values = true)]
        tags: Vec<String>,
        /// Body text, or '-' to read it from stdin.
        #[arg(long, value_name = "TEXT", allow_hyphen_values = true)]
        body: Option<String>,
    },
    /// List the open tasks of the resolved scope, grouped by project and feature.
    Ls {
        /// Scope options.
        #[command(flatten)]
        scope: ListScope,
        /// Only tasks of this feature.
        #[arg(
            short = 'f',
            long = "feature",
            value_name = "NAME",
            allow_hyphen_values = true
        )]
        feature: Option<String>,
        /// Only tasks in these statuses, separated by commas.
        #[arg(
            long,
            value_name = "STATUS",
            value_delimiter = ',',
            value_parser = parse_status,
            allow_hyphen_values = true,
            conflicts_with = "all"
        )]
        status: Vec<Status>,
        /// Only tasks with this tag.
        #[arg(long, value_name = "TAG", allow_hyphen_values = true)]
        tag: Option<String>,
        /// Include done and dropped tasks.
        #[arg(long)]
        all: bool,
        /// Show at most this many tasks, in display order.
        #[arg(long, value_name = "N", allow_hyphen_values = true)]
        limit: Option<usize>,
    },
    /// Show a task with all its notes.
    Show {
        /// Task id.
        id: i64,
    },
    /// Change a task; at least one option is required.
    Edit {
        /// Task id.
        id: i64,
        /// New title.
        #[arg(long, value_name = "TITLE", allow_hyphen_values = true)]
        title: Option<String>,
        /// New body text, or '-' to read it from stdin.
        #[arg(long, value_name = "TEXT", allow_hyphen_values = true)]
        body: Option<String>,
        /// New priority, 0 (most urgent) to 3.
        #[arg(long = "pri", value_name = "N", allow_hyphen_values = true)]
        priority: Option<u8>,
        /// Move the task to this feature, created when it does not exist.
        #[arg(
            short = 'f',
            long = "feature",
            value_name = "NAME",
            allow_hyphen_values = true,
            conflicts_with = "no_feature"
        )]
        feature: Option<String>,
        /// Remove the task from its feature.
        #[arg(long)]
        no_feature: bool,
        /// '+TAG' or 'TAG' adds a tag, '-TAG' removes one; repeat for several.
        #[arg(long = "tag", value_name = "[+|-]TAG", allow_hyphen_values = true)]
        tags: Vec<String>,
        /// Move the task to this project; its feature moves along by name.
        #[arg(
            short = 'p',
            long = "project",
            value_name = "NAME",
            allow_hyphen_values = true
        )]
        project: Option<String>,
    },
    /// The status and note commands.
    #[command(flatten)]
    Status(StatusCommand),
    /// Show the open task to work on next: by priority, then doing before todo, then age.
    Next {
        /// Scope options.
        #[command(flatten)]
        scope: ListScope,
        /// Only tasks of this feature.
        #[arg(
            short = 'f',
            long = "feature",
            value_name = "NAME",
            allow_hyphen_values = true
        )]
        feature: Option<String>,
    },
    /// Find tasks whose title, body or notes contain a text, ignoring case.
    Find {
        /// The text to find.
        query: String,
        /// Scope options.
        #[command(flatten)]
        scope: ListScope,
        /// Include done and dropped tasks.
        #[arg(long)]
        all: bool,
    },
    /// The digest, export and import commands.
    #[command(flatten)]
    Exchange(ExchangeCommand),
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
    /// Print the usage guide: variables, scope, every command, output, errors.
    Guide,
    /// Print the completion script for a shell.
    Completions {
        /// The shell.
        shell: clap_complete_command::Shell,
    },
}

/// The commands that summarize, export or import tasks.
#[derive(Debug, Subcommand)]
pub enum ExchangeCommand {
    /// Print a markdown digest of the open tasks, grouped by feature, blocked ones flagged.
    Brief {
        /// Scope options.
        #[command(flatten)]
        scope: ListScope,
    },
    /// Print every project, feature, task, tag and note, closed tasks included.
    Export {
        /// The document format.
        #[arg(long, value_enum, default_value_t = ExportFormat::Json)]
        format: ExportFormat,
        /// Only this project [default: every project, archived ones included].
        #[arg(
            short = 'p',
            long = "project",
            value_name = "NAME",
            allow_hyphen_values = true
        )]
        project: Option<String>,
    },
    /// Create tasks from JSON lines, all or none.
    Import {
        /// A file with one JSON object per line, or '-' for stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Scope options, for lines without a project.
        #[command(flatten)]
        scope: ProjectScope,
    },
}

/// The formats of `tk export`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ExportFormat {
    /// One JSON document.
    Json,
    /// Markdown headings and checkbox lists.
    Md,
}

/// The commands that change the status of tasks or append a note.
#[derive(Debug, Subcommand)]
pub enum StatusCommand {
    /// Start tasks: todo or blocked becomes doing.
    Start {
        /// Task ids.
        #[arg(required = true)]
        ids: Vec<i64>,
    },
    /// Block a task, with the reason stored as a note.
    Block {
        /// Task id.
        id: i64,
        /// Why the task is blocked.
        reason: String,
    },
    /// Finish tasks, with an optional note stored on each.
    Done {
        /// Task ids, then at most one note; a note that is a number needs --note.
        #[arg(value_name = "ID|NOTE", required = true)]
        args: Vec<String>,
        /// The note, given explicitly.
        #[arg(long, value_name = "TEXT", allow_hyphen_values = true)]
        note: Option<String>,
    },
    /// Drop a task, with the reason stored as a note.
    Drop {
        /// Task id.
        id: i64,
        /// Why the task will not be done.
        reason: String,
    },
    /// Reopen tasks: any other status becomes todo.
    Reopen {
        /// Task ids.
        #[arg(required = true)]
        ids: Vec<i64>,
    },
    /// Append a note to a task.
    Note {
        /// Task id.
        id: i64,
        /// The note.
        text: String,
    },
}

/// Scope options of commands that write to one project.
#[derive(Debug, Args)]
pub struct ProjectScope {
    /// The project [default: `TASKIST_PROJECT`, then the current directory].
    #[arg(
        short = 'p',
        long = "project",
        value_name = "NAME",
        allow_hyphen_values = true
    )]
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
    #[arg(
        short = 'p',
        long = "project",
        value_name = "NAME",
        allow_hyphen_values = true
    )]
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

fn parse_status(value: &str) -> Result<Status, crate::error::Error> {
    value.parse()
}

/// The `tk project` commands.
#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// Register a project.
    Add {
        /// Project name: lowercase letters, digits and '-'.
        name: String,
        /// Directory linked to the project; commands run inside it target the project.
        #[arg(long, value_name = "DIR", allow_hyphen_values = true)]
        path: Option<PathBuf>,
        /// Description.
        #[arg(long, value_name = "TEXT", allow_hyphen_values = true)]
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
        #[arg(long = "name", value_name = "NAME", allow_hyphen_values = true)]
        new_name: Option<String>,
        /// New linked directory.
        #[arg(
            long,
            value_name = "DIR",
            allow_hyphen_values = true,
            conflicts_with = "no_path"
        )]
        path: Option<PathBuf>,
        /// Remove the linked directory.
        #[arg(long)]
        no_path: bool,
        /// New description.
        #[arg(long, value_name = "TEXT", allow_hyphen_values = true)]
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
