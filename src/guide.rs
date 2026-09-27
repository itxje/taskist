//! The usage guide and the shell completion scripts; neither opens the database.

use clap::CommandFactory;
use clap_complete_command::Shell;
use serde::Serialize;

use crate::cli::Cli;
use crate::error::Error;

/// The guide, embedded at build time from `docs/guide.md`.
pub const GUIDE: &str = include_str!("../docs/guide.md");

/// A text document as the output shows it: `{text}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document {
    /// The document.
    pub text: String,
}

/// The usage guide.
///
/// ```
/// let guide = taskist::guide::guide();
/// assert!(guide.text.starts_with("# tk guide"));
/// assert!(guide.text.contains("## Exit codes"));
/// ```
pub fn guide() -> Document {
    Document {
        text: GUIDE.to_owned(),
    }
}

/// The completion script of `tk` for `shell`.
///
/// ```
/// use clap_complete_command::Shell;
///
/// let script = taskist::guide::completions(Shell::Bash)?;
/// assert!(script.text.contains("complete"));
/// # Ok::<(), taskist::error::Error>(())
/// ```
pub fn completions(shell: Shell) -> Result<Document, Error> {
    let mut script = Vec::new();
    shell.generate(&mut Cli::command(), &mut script);
    String::from_utf8(script)
        .map(|text| Document { text })
        .map_err(|err| Error::Internal(format!("the completion script is not UTF-8: {err}")))
}
