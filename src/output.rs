//! Rendering of results and errors, and the only place that writes to stdout or stderr.

use std::ffi::OsStr;
use std::io::Write;

use serde::Serialize;

use crate::env::Env;
use crate::error::Error;

/// How results and errors are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable text.
    Text,
    /// One JSON envelope per invocation.
    Json,
}

impl Format {
    /// Selects the format: `--json`, else `TASKIST_FORMAT` (`json` or `text`), else text.
    pub fn detect(json_flag: bool, taskist_format: Option<&OsStr>) -> Result<Self, Error> {
        if json_flag {
            return Ok(Self::Json);
        }
        match taskist_format {
            None => Ok(Self::Text),
            Some(value) if value == "json" => Ok(Self::Json),
            Some(value) if value == "text" => Ok(Self::Text),
            Some(value) => Err(Error::Usage(format!(
                "TASKIST_FORMAT must be json or text, got {:?}",
                value.to_string_lossy()
            ))),
        }
    }

    /// Selects the format from the `--json` flag and the captured environment.
    pub fn select(json_flag: bool, env: &Env) -> Result<Self, Error> {
        Self::detect(json_flag, env.var("TASKIST_FORMAT"))
    }
}

#[derive(Serialize)]
struct Success<'a, T> {
    ok: bool,
    data: &'a T,
}

#[derive(Serialize)]
struct Failure<'a> {
    ok: bool,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: String,
}

/// Renders the JSON success envelope `{"ok":true,"data":...}` as one line.
pub fn render_success<T: Serialize>(data: &T) -> Result<String, Error> {
    encode_line(&Success { ok: true, data })
}

/// Renders an error: the JSON failure envelope as one line, or `error: <message>`.
pub fn render_failure(format: Format, error: &Error) -> Result<String, Error> {
    match format {
        Format::Text => Ok(format!("error: {error}\n")),
        Format::Json => encode_line(&Failure {
            ok: false,
            error: ErrorBody {
                code: error.code(),
                message: error.to_string(),
            },
        }),
    }
}

fn encode_line(value: &impl Serialize) -> Result<String, Error> {
    let mut line = serde_json::to_string(value)
        .map_err(|err| Error::Internal(format!("cannot encode the output: {err}")))?;
    line.push('\n');
    Ok(line)
}

/// Writes plain text to stdout, as for `--help` and `--version`, and returns exit code 0.
pub fn emit_text(text: &str) -> u8 {
    match write_all(std::io::stdout().lock(), text) {
        Ok(()) => 0,
        Err(err) => emit_failure(Format::Text, &err),
    }
}

/// Writes a success result to stdout and returns exit code 0.
///
/// `human` is the text form; JSON output wraps `data` in the success envelope.
pub fn emit_success<T: Serialize>(format: Format, data: &T, human: &str) -> u8 {
    let text = match format {
        Format::Text => Ok(human.to_owned()),
        Format::Json => render_success(data),
    };
    match text.and_then(|text| write_all(std::io::stdout().lock(), &text)) {
        Ok(()) => 0,
        Err(err) => emit_failure(format, &err),
    }
}

/// Writes an error to stderr and returns its exit code.
pub fn emit_failure(format: Format, error: &Error) -> u8 {
    // A failure to report the error has nowhere left to go; the exit code still tells.
    let _ =
        render_failure(format, error).and_then(|text| write_all(std::io::stderr().lock(), &text));
    error.exit_code()
}

fn write_all(mut stream: impl Write, text: &str) -> Result<(), Error> {
    stream
        .write_all(text.as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|err| Error::Internal(format!("cannot write output: {err}")))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::PathBuf;

    use serde_json::{Value, json};

    use super::{Format, render_failure, render_success};
    use crate::env::Env;
    use crate::error::Error;

    #[test]
    fn json_flag_wins_over_the_variable() {
        assert_eq!(
            Format::detect(true, Some(OsStr::new("text"))).unwrap(),
            Format::Json
        );
        assert_eq!(
            Format::detect(true, Some(OsStr::new("xml"))).unwrap(),
            Format::Json
        );
    }

    #[test]
    fn variable_selects_json_or_text_and_defaults_to_text() {
        assert_eq!(Format::detect(false, None).unwrap(), Format::Text);
        assert_eq!(
            Format::detect(false, Some(OsStr::new("json"))).unwrap(),
            Format::Json
        );
        assert_eq!(
            Format::detect(false, Some(OsStr::new("text"))).unwrap(),
            Format::Text
        );
    }

    #[test]
    fn other_variable_values_are_usage_errors() {
        for value in ["xml", "", "JSON"] {
            let err = Format::detect(false, Some(OsStr::new(value))).unwrap_err();
            assert!(matches!(err, Error::Usage(_)), "{value}: {err:?}");
            assert!(err.to_string().contains("TASKIST_FORMAT"), "{err}");
        }
    }

    #[test]
    fn select_reads_the_captured_variable() {
        let env = Env::new(
            [("TASKIST_FORMAT".into(), "json".into())],
            PathBuf::from("/"),
            false,
        );
        assert_eq!(Format::select(false, &env).unwrap(), Format::Json);
    }

    #[test]
    fn success_envelope_is_one_line() {
        let line = render_success(&json!({"id": 1, "title": "a\nb"})).unwrap();
        assert_eq!(
            line,
            "{\"ok\":true,\"data\":{\"id\":1,\"title\":\"a\\nb\"}}\n"
        );
    }

    #[test]
    fn failure_envelope_is_one_line_with_code_and_message() {
        let line =
            render_failure(Format::Json, &Error::NotFound("no task 7\nsee ls".into())).unwrap();
        assert_eq!(line.lines().count(), 1);
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            value,
            json!({"ok": false, "error": {"code": "not_found", "message": "no task 7\nsee ls"}})
        );
    }

    #[test]
    fn human_failure_is_prefixed_with_error() {
        assert_eq!(
            render_failure(Format::Text, &Error::Conflict("project exists".into())).unwrap(),
            "error: project exists\n"
        );
    }
}
