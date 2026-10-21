//! Remote mode: `tk serve` runs commands forwarded by `tk` on other machines against its own
//! database, and `tk` forwards every command but `guide` and `completions` when
//! `TASKIST_SERVER` is set.
//!
//! One TCP connection carries one command. The client sends one JSON line, the [`Request`],
//! followed by `input_len` bytes of input: its stdin, or the file `tk import` names. The
//! server answers with one JSON line, the [`Reply`], and closes the connection. A request and
//! a reply are at most [`MAX_MESSAGE`] bytes each.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use clap::CommandFactory;
use serde::{Deserialize, Serialize};

use crate::cli::{Cli, Command, ExchangeCommand};
use crate::env::Env;
use crate::error::Error;
use crate::output::{self, Format};

/// The protocol version both sides send.
const VERSION: u32 = 1;
/// The largest request, header and input together, and the largest reply.
const MAX_MESSAGE: usize = 16 << 20;
/// How long a client tries to connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long either side waits for one read or write; longer than the store's busy timeout.
const IO_TIMEOUT: Duration = Duration::from_mins(1);
/// How many connections the server handles at once; further ones wait to be accepted.
const MAX_CONNECTIONS: usize = 32;

/// The variables a client forwards: every one the commands read, except those that locate the
/// database, which the server chooses for itself.
const FORWARDED_VARS: [&str; 9] = [
    "TASKIST_ACTOR",
    "TASKIST_PROJECT",
    "TASKIST_FORMAT",
    "USER",
    "NO_COLOR",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "TERM",
    "CI",
];

/// The header line of a request.
#[derive(Debug, Serialize, Deserialize)]
struct Request {
    /// The protocol version.
    v: u32,
    /// The shared token.
    token: String,
    /// The arguments, program name first.
    args: Vec<String>,
    /// The client's values of the [`FORWARDED_VARS`] it has set.
    vars: BTreeMap<String, String>,
    /// The client's canonical current directory.
    cwd: Cwd,
    /// Whether the client's stdout is a terminal.
    terminal: bool,
    /// How many bytes of input follow the header line.
    input_len: usize,
}

/// The client's current directory, or why it has none.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Cwd {
    /// The canonical path.
    Path(String),
    /// The message of the error met reading or resolving it.
    Error(String),
}

/// The reply line.
#[derive(Debug, Serialize, Deserialize)]
struct Reply {
    /// The protocol version.
    v: u32,
    /// What became of the request.
    #[serde(flatten)]
    outcome: Outcome,
}

/// What became of a request.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    /// The command ran: its exit code and what it wrote.
    Ran {
        /// The exit code.
        exit: u8,
        /// Everything written to stdout.
        stdout: String,
        /// Everything written to stderr.
        stderr: String,
    },
    /// The command did not run.
    Refused {
        /// Why.
        code: Refusal,
        /// The reason in words.
        message: String,
    },
}

/// Why a server refused a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Refusal {
    /// The token does not match.
    Unauthorized,
    /// The request is malformed, too large, or of another protocol version.
    BadRequest,
}

/// Forwards one parsed command to `server`, writes the streams the server returns and returns
/// the command's exit code; a failure to forward is reported in `format`.
pub(crate) fn forward(
    args: &[OsString],
    env: &Env,
    server: &OsStr,
    format: Format,
    command: &Command,
) -> u8 {
    let server = server.to_string_lossy();
    match request(args, env, command).and_then(|message| exchange(&server, &message)) {
        Ok((exit, stdout, stderr)) => output::emit_forwarded(&stdout, &stderr, exit),
        Err(err) => output::emit_failure(format, &err),
    }
}

/// The bytes of the request for one command: the header line, then the input.
fn request(args: &[OsString], env: &Env, command: &Command) -> Result<Vec<u8>, Error> {
    let token = env
        .var("TASKIST_TOKEN")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Error::Usage(
                "TASKIST_SERVER is set but TASKIST_TOKEN is not; set both to use a server".into(),
            )
        })?;
    let args = args
        .iter()
        .map(|arg| {
            arg.to_str().map(str::to_owned).ok_or_else(|| {
                Error::Usage(format!(
                    "the argument {} is not valid UTF-8, which a server needs",
                    Path::new(arg).display()
                ))
            })
        })
        .collect::<Result<_, _>>()?;
    let input = input(env, command)?;
    let vars = FORWARDED_VARS
        .iter()
        .filter_map(|name| {
            env.var(name)
                .map(|value| ((*name).to_owned(), value.to_string_lossy().into_owned()))
        })
        .collect();
    let cwd = match env.canonical_current_dir() {
        Ok(dir) => dir.into_os_string().into_string().map_or_else(
            |dir| {
                Cwd::Error(format!(
                    "the current directory {} is not valid UTF-8, which a server needs",
                    Path::new(&dir).display()
                ))
            },
            Cwd::Path,
        ),
        Err(err) => Cwd::Error(err.to_string()),
    };
    let header = Request {
        v: VERSION,
        token: token.to_string_lossy().into_owned(),
        args,
        vars,
        cwd,
        terminal: env.stdout_is_terminal(),
        input_len: input.len(),
    };
    let mut message = serde_json::to_vec(&header)
        .map_err(|err| Error::Internal(format!("cannot encode the request: {err}")))?;
    message.push(b'\n');
    message.extend(input);
    if message.len() > MAX_MESSAGE {
        return Err(Error::Usage(format!(
            "the command and its input are larger than the {} MiB a server accepts",
            MAX_MESSAGE >> 20
        )));
    }
    Ok(message)
}

/// What the command reads in place of stdin: the body for a `-` body, the import file or
/// stdin for `tk import`, nothing otherwise. Stdin is read only when the command reads it.
fn input(env: &Env, command: &Command) -> Result<Vec<u8>, Error> {
    match command {
        Command::Add {
            body: Some(body), ..
        }
        | Command::Edit {
            body: Some(body), ..
        } if body == "-" => env
            .read_stdin()
            .map_err(|err| Error::Internal(format!("cannot read the body from stdin: {err}"))),
        Command::Exchange(ExchangeCommand::Import { file, .. }) => {
            crate::command::exchange::read_source(env, file)
        }
        _ => Ok(Vec::new()),
    }
}

/// Sends `message` to `server` and returns the exit code and streams of the command it ran.
fn exchange(server: &str, message: &[u8]) -> Result<(u8, String, String), Error> {
    let stream = connect(server)?;
    let lost = |err: std::io::Error| {
        Error::Unreachable(format!(
            "the connection to the server {server} failed: {err}"
        ))
    };
    stream.set_read_timeout(Some(IO_TIMEOUT)).map_err(lost)?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).map_err(lost)?;
    (&stream)
        .write_all(message)
        .and_then(|()| (&stream).flush())
        .map_err(lost)?;
    let line =
        read_line(&mut BufReader::new((&stream).take(MAX_MESSAGE as u64))).map_err(|problem| {
            Error::Unreachable(format!("the reply of the server {server} {problem}"))
        })?;
    let reply: Reply = serde_json::from_slice(&line).map_err(|err| {
        Error::Unreachable(format!(
            "the reply of the server {server} is malformed: {err}"
        ))
    })?;
    if reply.v != VERSION {
        return Err(Error::Unreachable(format!(
            "the server {server} speaks protocol version {}, not {VERSION}",
            reply.v
        )));
    }
    match reply.outcome {
        Outcome::Ran {
            exit,
            stdout,
            stderr,
        } => Ok((exit, stdout, stderr)),
        Outcome::Refused {
            code: Refusal::Unauthorized,
            message,
        } => Err(Error::Unauthorized(format!(
            "the server {server} {message}"
        ))),
        Outcome::Refused {
            code: Refusal::BadRequest,
            message,
        } => Err(Error::Internal(format!(
            "the server {server} refused the request: {message}"
        ))),
    }
}

/// A connection to the first address of `server` that accepts one.
fn connect(server: &str) -> Result<TcpStream, Error> {
    let addresses = server
        .to_socket_addrs()
        .map_err(|err| Error::Unreachable(format!("cannot resolve the server {server}: {err}")))?;
    let mut failure = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(err) => failure = Some(err),
        }
    }
    Err(Error::Unreachable(failure.map_or_else(
        || format!("the server {server} resolves to no address"),
        |err| format!("cannot connect to the server {server}: {err}"),
    )))
}

/// One line read from `reader`, newline included, which bounds how much it reads; `Err` says
/// what is wrong with it, as the end of a sentence.
fn read_line(reader: &mut impl BufRead) -> Result<Vec<u8>, String> {
    let mut line = Vec::new();
    reader
        .read_until(b'\n', &mut line)
        .map_err(|err| format!("could not be read: {err}"))?;
    if line.ends_with(b"\n") {
        Ok(line)
    } else if line.len() >= MAX_MESSAGE {
        Err(format!("is larger than {} MiB", MAX_MESSAGE >> 20))
    } else {
        Err("ended before its end of line".to_owned())
    }
}

/// `tk serve`: serves the database of `env` on `listen` to forwarding clients, and returns only
/// when it cannot start.
pub fn serve(env: &Env, listen: &str) -> Result<(), Error> {
    let token: Arc<str> = env
        .var("TASKIST_TOKEN")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Error::Usage("tk serve needs TASKIST_TOKEN, the token clients must send".into())
        })?
        .to_string_lossy()
        .into();
    let listener = TcpListener::bind(listen)
        .map_err(|err| Error::Usage(format!("cannot listen on {listen}: {err}")))?;
    let address = listener
        .local_addr()
        .map_err(|err| Error::Internal(format!("cannot read the listening address: {err}")))?;
    output::log(&format!("listening on {address}"));
    let base = Arc::new(env.clone());
    let slots = Arc::new((Mutex::new(0), Condvar::new()));
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(err) => {
                output::log(&format!("cannot accept a connection: {err}"));
                continue;
            }
        };
        let slot = Slot::take(&slots);
        let (base, token) = (Arc::clone(&base), Arc::clone(&token));
        let spawned = std::thread::Builder::new().spawn(move || {
            let _slot = slot;
            handle(&stream, &base, &token);
        });
        if let Err(err) = spawned {
            output::log(&format!("cannot start a connection thread: {err}"));
        }
    }
    Ok(())
}

/// One of the [`MAX_CONNECTIONS`] connection slots, given back when dropped.
#[derive(Debug)]
struct Slot(Arc<(Mutex<usize>, Condvar)>);

impl Slot {
    /// Waits until fewer than [`MAX_CONNECTIONS`] slots are taken, then takes one.
    fn take(slots: &Arc<(Mutex<usize>, Condvar)>) -> Self {
        let (taken, freed) = &**slots;
        *freed
            .wait_while(
                taken.lock().unwrap_or_else(PoisonError::into_inner),
                |taken| *taken >= MAX_CONNECTIONS,
            )
            .unwrap_or_else(PoisonError::into_inner) += 1;
        Self(Arc::clone(slots))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let (taken, freed) = &*self.0;
        *taken.lock().unwrap_or_else(PoisonError::into_inner) -= 1;
        freed.notify_one();
    }
}

/// Answers one connection and logs one line for it.
fn handle(stream: &TcpStream, base: &Env, token: &str) {
    let started = Instant::now();
    let peer = stream
        .peer_addr()
        .map_or_else(|_| "an unknown peer".to_owned(), |peer| peer.to_string());
    let (reply, summary) = answer(stream, base, token);
    let sent = serde_json::to_vec(&reply)
        .map_err(|err| err.to_string())
        .and_then(|mut line| {
            line.push(b'\n');
            let mut writer = stream;
            writer
                .write_all(&line)
                .and_then(|()| writer.flush())
                .map_err(|err| err.to_string())
        });
    let elapsed = started.elapsed().as_millis();
    match sent {
        Ok(()) => output::log(&format!("{peer} {summary} in {elapsed} ms")),
        Err(err) => output::log(&format!(
            "{peer} {summary} in {elapsed} ms; the reply was not sent: {err}"
        )),
    }
}

/// The reply to the request on `stream`, and a summary for the log that holds neither the
/// token nor any text of the command.
fn answer(stream: &TcpStream, base: &Env, token: &str) -> (Reply, String) {
    let refuse = |code: Refusal, message: String| {
        let summary = format!(
            "refused ({}): {message}",
            if code == Refusal::Unauthorized {
                "unauthorized"
            } else {
                "bad request"
            }
        );
        let outcome = Outcome::Refused { code, message };
        (
            Reply {
                v: VERSION,
                outcome,
            },
            summary,
        )
    };
    let timeouts = stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(IO_TIMEOUT)));
    if let Err(err) = timeouts {
        return refuse(Refusal::BadRequest, format!("cannot set timeouts: {err}"));
    }
    let mut reader = BufReader::new(stream.take(MAX_MESSAGE as u64));
    let header = match read_line(&mut reader) {
        Ok(header) => header,
        Err(problem) => return refuse(Refusal::BadRequest, format!("the request {problem}")),
    };
    let request: Request = match serde_json::from_slice(&header) {
        Ok(request) => request,
        Err(err) => {
            return refuse(
                Refusal::BadRequest,
                format!("the request is malformed: {err}"),
            );
        }
    };
    if request.v != VERSION {
        return refuse(
            Refusal::BadRequest,
            format!("protocol version {} is not {VERSION}", request.v),
        );
    }
    if !same_token(&request.token, token) {
        return refuse(Refusal::Unauthorized, "refused the token".to_owned());
    }
    if request.input_len > MAX_MESSAGE - header.len() {
        return refuse(
            Refusal::BadRequest,
            format!("the request is larger than {} MiB", MAX_MESSAGE >> 20),
        );
    }
    let mut input = vec![0; request.input_len];
    if let Err(err) = reader.read_exact(&mut input) {
        return refuse(
            Refusal::BadRequest,
            format!("the input could not be read: {err}"),
        );
    }
    let args: Vec<OsString> = request.args.into_iter().map(OsString::from).collect();
    let env = request_env(base, request.vars, request.cwd, request.terminal, input);
    let name = command_name(&args);
    let (exit, captured) = output::capture(|| {
        if name == "serve" {
            crate::fail(
                &args,
                env.var("TASKIST_FORMAT"),
                &Error::Usage("tk serve cannot be forwarded to a server".into()),
            )
        } else {
            crate::run_here(&args, &env)
        }
    });
    let outcome = Outcome::Ran {
        exit,
        stdout: captured.stdout,
        stderr: captured.stderr,
    };
    (
        Reply {
            v: VERSION,
            outcome,
        },
        format!("{name} exit {exit}"),
    )
}

/// Whether two tokens are equal, in a time that depends on their lengths only.
fn same_token(given: &str, expected: &str) -> bool {
    let (given, expected) = (given.as_bytes(), expected.as_bytes());
    given.len() == expected.len()
        && given
            .iter()
            .zip(expected)
            .fold(0, |difference, (a, b)| difference | (a ^ b))
            == 0
}

/// The environment of a forwarded command: the server's variables, except the forwarded ones,
/// which are the client's, with the client's directory, terminal state and input.
fn request_env(
    base: &Env,
    vars: BTreeMap<String, String>,
    cwd: Cwd,
    terminal: bool,
    input: Vec<u8>,
) -> Env {
    let forwarded = |name: &OsStr| FORWARDED_VARS.iter().any(|forwarded| name == *forwarded);
    let mut all: BTreeMap<OsString, OsString> = base
        .vars()
        .iter()
        .filter(|(name, _)| !forwarded(name))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    all.extend(
        vars.into_iter()
            .map(|(name, value)| (OsString::from(name), OsString::from(value)))
            .filter(|(name, _)| forwarded(name)),
    );
    let cwd = match cwd {
        Cwd::Path(path) => Ok(PathBuf::from(path)),
        Cwd::Error(message) => Err(message),
    };
    Env::forwarded(all, cwd, terminal, input)
}

/// The subcommand path of `args`, such as `project add`, or `(unparsed)`.
fn command_name(args: &[OsString]) -> String {
    let Ok(matches) = Cli::command().try_get_matches_from(args) else {
        return "(unparsed)".to_owned();
    };
    let mut names = Vec::new();
    let mut current = &matches;
    while let Some((name, sub)) = current.subcommand() {
        names.push(name);
        current = sub;
    }
    names.join(" ")
}

#[cfg(test)]
mod tests {
    use super::{Cwd, Outcome, Refusal, Reply, request_env, same_token};
    use crate::env::Env;
    use std::path::PathBuf;

    #[test]
    fn tokens_match_only_when_equal() {
        assert!(same_token("abc", "abc"));
        assert!(!same_token("abx", "abc"));
        assert!(!same_token("ab", "abc"));
        assert!(!same_token("", "abc"));
    }

    #[test]
    fn the_server_keeps_its_database_variables_and_takes_the_forwarded_ones() {
        let base = Env::new(
            [
                ("TASKIST_DB".into(), "/srv/tk.db".into()),
                ("USER".into(), "server".into()),
                ("TERM".into(), "dumb".into()),
            ],
            PathBuf::from("/srv"),
            false,
        );
        let vars = [
            ("TASKIST_DB", "/client/tk.db"),
            ("HOME", "/client"),
            ("USER", "client"),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
        let env = request_env(&base, vars, Cwd::Path("/work".into()), true, b"in".to_vec());
        assert_eq!(env.var("TASKIST_DB"), Some("/srv/tk.db".as_ref()));
        assert_eq!(env.var("HOME"), None);
        assert_eq!(env.var("USER"), Some("client".as_ref()));
        assert_eq!(
            env.var("TERM"),
            None,
            "a forwarded variable the client lacks is unset"
        );
        assert_eq!(env.current_dir().unwrap(), PathBuf::from("/work"));
        assert_eq!(env.canonical_current_dir().unwrap(), PathBuf::from("/work"));
        assert!(env.stdout_is_terminal());
        assert_eq!(env.read_stdin().unwrap(), b"in");
    }

    #[test]
    fn a_client_directory_error_is_reported_when_a_command_needs_the_directory() {
        let base = Env::new([], PathBuf::from("/srv"), false);
        let env = request_env(
            &base,
            [].into(),
            Cwd::Error("gone".into()),
            false,
            Vec::new(),
        );
        assert_eq!(env.current_dir().unwrap_err().to_string(), "gone");
        assert_eq!(env.canonical_current_dir().unwrap_err().to_string(), "gone");
    }

    #[test]
    fn replies_read_back_as_written() {
        let reply = Reply {
            v: 1,
            outcome: Outcome::Refused {
                code: Refusal::Unauthorized,
                message: "refused the token".into(),
            },
        };
        let text = serde_json::to_string(&reply).unwrap();
        assert_eq!(
            text,
            r#"{"v":1,"refused":{"code":"unauthorized","message":"refused the token"}}"#
        );
        let back: Reply = serde_json::from_str(&text).unwrap();
        assert!(matches!(
            back.outcome,
            Outcome::Refused {
                code: Refusal::Unauthorized,
                ..
            }
        ));
    }
}
