//! Concurrent writers: separate `tk` processes adding tasks to one database at once.
#![cfg(test)]

pub mod common;

use std::collections::BTreeSet;
use std::process::Output;
use std::sync::Barrier;
use std::time::Instant;

use common::Sandbox;

/// Writer loops that run at the same time.
const WRITERS: usize = 2;
/// Consecutive `tk add` processes per writer.
const ADDS_PER_WRITER: usize = 50;

/// The stderr lines of a run that report a locked or busy database.
fn lock_complaints(stderr: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter(|line| {
            let line = line.to_lowercase();
            line.contains("locked") || line.contains("busy")
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_lock_complaint_reader_recognizes_the_sqlite_messages() {
    // Control: the messages SQLite gives for SQLITE_LOCKED and SQLITE_BUSY are found.
    assert_eq!(
        lock_complaints(b"error: database error: database is locked\nok\n"),
        ["error: database error: database is locked"]
    );
    assert_eq!(
        lock_complaints(b"{\"message\":\"database error: Database Busy\"}\n").len(),
        1
    );
    assert!(lock_complaints(b"error: no task 7\n").is_empty());
}

#[test]
fn concurrent_writer_processes_lose_no_task_and_report_no_lock_error() {
    let sandbox = Sandbox::new();
    sandbox.ok(&["project", "add", "web"]);

    let barrier = Barrier::new(WRITERS);
    let runs: Vec<(Instant, Instant, Vec<Output>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..WRITERS)
            .map(|writer| {
                let (sandbox, barrier) = (&sandbox, &barrier);
                scope.spawn(move || {
                    barrier.wait();
                    let start = Instant::now();
                    let outputs = (0..ADDS_PER_WRITER)
                        .map(|n| {
                            sandbox
                                .tk()
                                .args(["--json", "add", "-p", "web"])
                                .arg(format!("writer {writer} task {n}"))
                                .output()
                                .expect("run tk")
                        })
                        .collect();
                    (start, Instant::now(), outputs)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("writer thread"))
            .collect()
    });

    // The writer loops overlapped in time, so their processes competed for the database.
    let last_start = runs.iter().map(|(start, _, _)| *start).max().expect("runs");
    let first_end = runs.iter().map(|(_, end, _)| *end).min().expect("runs");
    assert!(last_start < first_end, "the writer loops did not overlap");

    let mut reported = BTreeSet::new();
    for (_, _, outputs) in &runs {
        assert_eq!(outputs.len(), ADDS_PER_WRITER);
        for output in outputs {
            assert_eq!(
                lock_complaints(&output.stderr),
                Vec::<String>::new(),
                "{output:?}"
            );
            let data = common::ok_data(output);
            reported.insert(data["task"]["id"].as_i64().expect("task id"));
        }
    }
    assert_eq!(reported.len(), WRITERS * ADDS_PER_WRITER);

    // The database holds exactly the reported tasks.
    let list = sandbox.ok(&["ls", "-p", "web"]);
    let stored: BTreeSet<i64> = list["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|task| task["id"].as_i64().expect("task id"))
        .collect();
    assert_eq!(stored, reported);
    let titles: BTreeSet<String> = list["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|task| task["title"].as_str().expect("title").to_owned())
        .collect();
    assert_eq!(titles.len(), WRITERS * ADDS_PER_WRITER);
}
