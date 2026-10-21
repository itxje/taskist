//! Remote mode: `tk serve` runs forwarded commands against its database, and `tk` with
//! `TASKIST_SERVER` forwards to it with the output and exit code of a local run.
#![cfg(test)]

pub mod common;

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Output, Stdio};

use common::{Sandbox, coverage_vars};
use serde_json::Value;

const TOKEN: &str = "s3cret-token";

/// A running `tk serve` on a free loopback port, with its own sandbox database.
struct Server {
    sandbox: Sandbox,
    child: Child,
    address: String,
}

impl Server {
    fn start() -> Self {
        Self::start_with(&[("TASKIST_TOKEN", TOKEN)]).expect("server starts")
    }

    /// Starts `tk serve --listen 127.0.0.1:0` with `extra` variables; `Err` with the
    /// child's output when it exits before reporting its address.
    fn start_with(extra: &[(&str, &str)]) -> Result<Self, Output> {
        let sandbox = Sandbox::new();
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("tk"));
        cmd.env_clear()
            .envs(sandbox.vars())
            .envs(coverage_vars())
            .envs(
                extra
                    .iter()
                    .map(|(k, v)| (OsString::from(k), OsString::from(v))),
            )
            .current_dir(sandbox.work())
            .args(["serve", "--listen", "127.0.0.1:0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn tk serve");
        let stderr = child.stderr.take().expect("server stderr");
        let mut lines = BufReader::new(stderr).lines();
        let first = lines.next().and_then(Result::ok);
        let address = first
            .as_deref()
            .and_then(|line| line.strip_prefix("tk serve: listening on "))
            .map(str::to_owned);
        if let Some(address) = address {
            // Keep draining the request log so the server never blocks on stderr.
            std::thread::spawn(move || lines.for_each(drop));
            return Ok(Self {
                sandbox,
                child,
                address,
            });
        }
        let mut output = child.wait_with_output().expect("collect server output");
        if let Some(line) = first {
            output.stderr.splice(0..0, format!("{line}\n").into_bytes());
        }
        Err(output)
    }

    /// A client command in `client`'s sandbox, forwarding to this server.
    fn client(&self, client: &Sandbox) -> assert_cmd::Command {
        let mut cmd = client.tk();
        cmd.env("TASKIST_SERVER", &self.address)
            .env("TASKIST_TOKEN", TOKEN);
        cmd
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

fn seed(server: &Server) {
    let local = &server.sandbox;
    local.ok(&["project", "add", "web"]);
    local.ok(&[
        "add",
        "Fix login",
        "-p",
        "web",
        "-f",
        "auth",
        "--pri",
        "1",
        "--tag",
        "bug",
    ]);
    local.ok(&["add", "Write docs", "-p", "web"]);
    local.ok(&["block", "2", "waiting for review", "--by", "seed"]);
}

#[test]
fn read_commands_print_exactly_what_a_local_run_prints() {
    let server = Server::start();
    seed(&server);
    let client = Sandbox::new();
    let reads: &[&[&str]] = &[
        &["ls", "--all-projects"],
        &["ls", "-p", "web", "--all"],
        &["show", "1"],
        &["next", "-p", "web"],
        &["find", "login"],
        &["project", "ls"],
        &["project", "show", "web"],
        &["feature", "ls", "-p", "web"],
        &["brief", "-p", "web"],
        &["show", "99"],
        &["ls", "-p", "nowhere"],
    ];
    for args in reads {
        for json_flag in [false, true] {
            let mut local = server.sandbox.tk();
            let mut remote = server.client(&client);
            if json_flag {
                local.arg("--json");
                remote.arg("--json");
            }
            let local = local.args(*args).output().expect("run local tk");
            let remote = remote.args(*args).output().expect("run remote tk");
            assert_eq!(
                remote.status.code(),
                local.status.code(),
                "exit code of {args:?}"
            );
            assert_eq!(
                remote.stdout, local.stdout,
                "stdout of {args:?} json={json_flag}"
            );
            assert_eq!(
                remote.stderr, local.stderr,
                "stderr of {args:?} json={json_flag}"
            );
        }
    }
    // An export differs only in the time it was taken.
    let export = |mut cmd: assert_cmd::Command| {
        let out = cmd.arg("export").output().expect("run tk export");
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let mut document = json(&out);
        document["exported_at"] = Value::Null;
        document
    };
    assert_eq!(export(server.client(&client)), export(server.sandbox.tk()));
}

#[test]
fn writes_land_in_the_server_database_and_never_in_the_client_one() {
    let server = Server::start();
    server.sandbox.ok(&["project", "add", "web"]);
    let client = Sandbox::new();
    let out = server
        .client(&client)
        .args([
            "--json",
            "add",
            "Remote task",
            "-p",
            "web",
            "--by",
            "agent:remote",
        ])
        .output()
        .expect("run remote add");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(json(&out)["data"]["task"]["title"], "Remote task");
    assert_eq!(json(&out)["data"]["task"]["created_by"], "agent:remote");
    let tasks = server.sandbox.ok(&["ls", "-p", "web"]);
    assert_eq!(tasks["tasks"][0]["title"], "Remote task");
    assert!(
        !client.db().exists(),
        "the client database must not be created"
    );
}

#[test]
fn the_client_database_variable_is_never_forwarded() {
    let server = Server::start();
    server.sandbox.ok(&["project", "add", "web"]);
    let client = Sandbox::new();
    // The client sandbox sets TASKIST_DB to its own file; the server keeps its own.
    server
        .client(&client)
        .args(["add", "Only on the server", "-p", "web"])
        .assert()
        .success();
    assert!(!client.db().exists());
    assert_eq!(
        server.sandbox.ok(&["ls", "-p", "web"])["tasks"][0]["title"],
        "Only on the server"
    );
}

#[test]
fn the_client_directory_and_user_choose_the_project_and_the_actor() {
    let server = Server::start();
    let client = Sandbox::new();
    let linked = client.work().to_str().expect("utf-8 path").to_owned();
    server
        .sandbox
        .ok(&["project", "add", "web", "--path", &linked]);
    server
        .client(&client)
        .env("USER", "ada")
        .args(["add", "Picked by directory"])
        .assert()
        .success();
    let task = &server.sandbox.ok(&["show", "1"])["task"];
    assert_eq!(task["project"], "web");
    assert_eq!(task["created_by"], "ada");
}

#[test]
fn a_body_read_from_stdin_is_forwarded() {
    let server = Server::start();
    server.sandbox.ok(&["project", "add", "web"]);
    let client = Sandbox::new();
    server
        .client(&client)
        .args(["add", "With body", "-p", "web", "--body", "-"])
        .write_stdin("line one\nline two\n")
        .assert()
        .success();
    assert_eq!(
        server.sandbox.ok(&["show", "1"])["task"]["body"],
        "line one\nline two\n"
    );
}

#[test]
fn an_import_file_on_the_client_is_imported_on_the_server() {
    let server = Server::start();
    server.sandbox.ok(&["project", "add", "web"]);
    let client = Sandbox::new();
    std::fs::write(
        client.work().join("tasks.jsonl"),
        "{\"title\":\"Imported one\",\"project\":\"web\"}\n{\"title\":\"Imported two\",\"project\":\"web\"}\n",
    )
    .expect("write import file");
    let out = server
        .client(&client)
        .args(["import", "tasks.jsonl"])
        .output()
        .expect("run remote import");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let titles: Vec<Value> = server.sandbox.ok(&["ls", "-p", "web"])["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|task| task["title"].clone())
        .collect();
    assert_eq!(titles, ["Imported one", "Imported two"]);
}

#[test]
fn a_wrong_token_is_unauthorized_and_writes_nothing() {
    let server = Server::start();
    server.sandbox.ok(&["project", "add", "web"]);
    let client = Sandbox::new();
    let out = server
        .client(&client)
        .env("TASKIST_TOKEN", "wrong")
        .args(["--json", "add", "Should not exist", "-p", "web"])
        .output()
        .expect("run remote add");
    assert_eq!(out.status.code(), Some(6));
    let error: Value = serde_json::from_slice(&out.stderr).expect("stderr is JSON");
    assert_eq!(error["error"]["code"], "unauthorized");
    assert_eq!(
        server.sandbox.ok(&["ls", "-p", "web"])["tasks"],
        Value::Array(vec![])
    );
}

#[test]
fn a_server_without_a_token_variable_refuses_to_start() {
    let output = Server::start_with(&[])
        .err()
        .expect("tk serve must refuse to start");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("TASKIST_TOKEN"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_client_without_a_token_is_a_usage_error() {
    let client = Sandbox::new();
    let out = client
        .tk()
        .env("TASKIST_SERVER", "127.0.0.1:9")
        .args(["--json", "ls", "--all-projects"])
        .output()
        .expect("run tk");
    assert_eq!(out.status.code(), Some(2));
    let error: Value = serde_json::from_slice(&out.stderr).expect("stderr is JSON");
    assert_eq!(error["error"]["code"], "usage");
}

#[test]
fn an_unreachable_server_is_its_own_error() {
    let client = Sandbox::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address").to_string();
    drop(listener);
    let out = client
        .tk()
        .env("TASKIST_SERVER", &address)
        .env("TASKIST_TOKEN", TOKEN)
        .args(["--json", "ls", "--all-projects"])
        .output()
        .expect("run tk");
    assert_eq!(out.status.code(), Some(5));
    let error: Value = serde_json::from_slice(&out.stderr).expect("stderr is JSON");
    assert_eq!(error["error"]["code"], "unreachable");
}

#[test]
fn usage_errors_guide_and_help_stay_local() {
    let client = Sandbox::new();
    let dead = |args: &[&str]| {
        client
            .tk()
            .env("TASKIST_SERVER", "127.0.0.1:9")
            .env("TASKIST_TOKEN", TOKEN)
            .args(args)
            .output()
            .expect("run tk")
    };
    let local = |args: &[&str]| client.tk().args(args).output().expect("run tk");
    for args in [
        &["frobnicate"][..],
        &["guide"],
        &["--help"],
        &["--version"],
        &["completions", "bash"],
    ] {
        let (remote, local) = (dead(args), local(args));
        assert_eq!(remote.status.code(), local.status.code(), "{args:?}");
        assert_eq!(remote.stdout, local.stdout, "{args:?}");
        assert_eq!(remote.stderr, local.stderr, "{args:?}");
    }
}

#[test]
fn a_forwarded_serve_is_refused() {
    let server = Server::start();
    let client = Sandbox::new();
    let out = server
        .client(&client)
        .args(["--json", "serve", "--listen", "127.0.0.1:0"])
        .output()
        .expect("run remote serve");
    assert_eq!(out.status.code(), Some(2));
    let error: Value = serde_json::from_slice(&out.stderr).expect("stderr is JSON");
    assert_eq!(error["error"]["code"], "usage");
}

#[test]
fn an_oversized_request_is_refused_and_the_server_keeps_serving() {
    let server = Server::start();
    server.sandbox.ok(&["project", "add", "web"]);
    let mut stream = TcpStream::connect(&server.address).expect("connect");
    let chunk = vec![b'x'; 1 << 20];
    for _ in 0..17 {
        if stream.write_all(&chunk).is_err() {
            break;
        }
    }
    let _ = stream.write_all(b"\n");
    let mut reply = String::new();
    let _ = stream.read_to_string(&mut reply);
    assert!(
        !reply.contains("\"exit\""),
        "an oversized request must not run: {reply}"
    );
    let client = Sandbox::new();
    server
        .client(&client)
        .args(["ls", "-p", "web"])
        .assert()
        .success();
}

#[test]
fn concurrent_forwarded_writers_lose_nothing() {
    let server = Server::start();
    server.sandbox.ok(&["project", "add", "web"]);
    let clients: Vec<Sandbox> = (0..4).map(|_| Sandbox::new()).collect();
    std::thread::scope(|scope| {
        for (writer, client) in clients.iter().enumerate() {
            let server = &server;
            scope.spawn(move || {
                for n in 0..10 {
                    server
                        .client(client)
                        .args(["add", &format!("writer {writer} task {n}"), "-p", "web"])
                        .assert()
                        .success();
                }
            });
        }
    });
    let tasks = server.sandbox.ok(&["ls", "-p", "web", "--limit", "100"]);
    assert_eq!(tasks["tasks"].as_array().expect("tasks").len(), 40);
}
