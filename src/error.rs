//! Typed errors and their mapping to error codes and process exit codes.

/// Every failure the library reports.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Unexpected failure: I/O, the database engine, corrupt data.
    #[error("{0}")]
    Internal(String),
    /// The database was written by a newer version of the tool.
    #[error("database schema version {found} is newer than the supported version {supported}")]
    UnsupportedSchema {
        /// Schema version stored in the database.
        found: i64,
        /// Highest schema version this binary supports.
        supported: i64,
    },
    /// Bad arguments, bad values, missing scope, malformed import line.
    #[error("{0}")]
    Usage(String),
    /// Unknown task id, project or feature.
    #[error("{0}")]
    NotFound(String),
    /// Duplicate name or path, archived project, non-empty project removal.
    #[error("{0}")]
    Conflict(String),
    /// Status change the transition table forbids.
    #[error("{0}")]
    InvalidTransition(String),
}

impl Error {
    /// The stable error code reported in the JSON error envelope.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Internal(_) => "internal",
            Self::UnsupportedSchema { .. } => "unsupported_schema",
            Self::Usage(_) => "usage",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::InvalidTransition(_) => "invalid_transition",
        }
    }

    /// The process exit code for this error.
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Internal(_) | Self::UnsupportedSchema { .. } => 1,
            Self::Usage(_) => 2,
            Self::NotFound(_) => 3,
            Self::Conflict(_) | Self::InvalidTransition(_) => 4,
        }
    }
}

impl From<clap::Error> for Error {
    /// An argument parsing failure is a usage error carrying clap's text without its `error: ` prefix.
    fn from(err: clap::Error) -> Self {
        let text = err.to_string();
        let message = text.strip_prefix("error: ").unwrap_or(&text).trim_end();
        Self::Usage(message.to_owned())
    }
}

impl From<rusqlite::Error> for Error {
    /// A violated uniqueness is a conflict, a violated check a usage error, a missing
    /// referenced row not found; every other database failure is internal.
    fn from(err: rusqlite::Error) -> Self {
        use rusqlite::ffi;

        let message = err.to_string();
        match err.sqlite_error() {
            Some(sqlite) => match sqlite.extended_code {
                ffi::SQLITE_CONSTRAINT_UNIQUE | ffi::SQLITE_CONSTRAINT_PRIMARYKEY => {
                    Self::Conflict(message)
                }
                ffi::SQLITE_CONSTRAINT_CHECK => Self::Usage(message),
                ffi::SQLITE_CONSTRAINT_FOREIGNKEY => Self::NotFound(message),
                _ => Self::Internal(format!("database error: {message}")),
            },
            None => Self::Internal(format!("database error: {message}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    /// One value of every variant; the exhaustive match keeps the list complete.
    fn every_variant() -> Vec<Error> {
        let all = vec![
            Error::Internal("disk full".into()),
            Error::UnsupportedSchema {
                found: 9,
                supported: 1,
            },
            Error::Usage("bad flag".into()),
            Error::NotFound("no task 7".into()),
            Error::Conflict("project exists".into()),
            Error::InvalidTransition("task is dropped".into()),
        ];
        for error in &all {
            match error {
                Error::Internal(_)
                | Error::UnsupportedSchema { .. }
                | Error::Usage(_)
                | Error::NotFound(_)
                | Error::Conflict(_)
                | Error::InvalidTransition(_) => {}
            }
        }
        all
    }

    #[test]
    fn every_variant_maps_to_its_documented_code_and_exit_code() {
        let mapped: Vec<(&str, u8)> = every_variant()
            .iter()
            .map(|error| (error.code(), error.exit_code()))
            .collect();
        assert_eq!(
            mapped,
            [
                ("internal", 1),
                ("unsupported_schema", 1),
                ("usage", 2),
                ("not_found", 3),
                ("conflict", 4),
                ("invalid_transition", 4),
            ]
        );
    }

    #[test]
    fn messages_are_the_given_text() {
        assert_eq!(Error::NotFound("no task 7".into()).to_string(), "no task 7");
        assert_eq!(
            Error::UnsupportedSchema {
                found: 9,
                supported: 1
            }
            .to_string(),
            "database schema version 9 is newer than the supported version 1"
        );
    }

    #[test]
    fn database_failures_other_than_constraints_are_internal() {
        let err = Error::from(rusqlite::Error::QueryReturnedNoRows);
        assert!(matches!(err, Error::Internal(_)), "{err:?}");
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let err = Error::from(conn.execute("DELETE FROM missing_table", []).unwrap_err());
        assert!(matches!(err, Error::Internal(_)), "{err:?}");
        assert!(err.to_string().starts_with("database error: "), "{err}");
    }
}
