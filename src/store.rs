//! The database store: connection setup, embedded migrations and transactions.
//!
//! Every command runs in exactly one transaction: [`Store::write`] starts it with
//! `BEGIN IMMEDIATE`, [`Store::read`] with a deferred `BEGIN`. The queries live on [`Tx`].

mod query;

use std::fs::DirBuilder;
use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::{Connection, TransactionBehavior};

use crate::error::Error;

pub use query::{NewTask, TaskUpdate};

/// How long a statement waits for a lock held by another process.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Embedded migrations; migration `n` is element `n - 1` and brings the schema to version `n`.
/// A schema change is a new migration appended here, never an edit of an existing one.
const MIGRATIONS: [&str; 1] = [include_str!("store/migrations/0001_initial.sql")];

/// The schema version this binary creates and understands.
pub const SCHEMA_VERSION: i64 = 1;

/// An open database.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

/// One transaction, committed only when the closure that uses it succeeds.
#[derive(Debug)]
pub struct Tx<'c> {
    tx: rusqlite::Transaction<'c>,
}

impl Store {
    /// Opens the database at `path`, creating it and its missing parent directories.
    ///
    /// Sets, in this order, the busy timeout, WAL journal mode, foreign keys and
    /// `synchronous = NORMAL`, then applies pending migrations. A database with a newer
    /// schema is refused with [`Error::UnsupportedSchema`] before anything is written to it.
    ///
    /// ```
    /// use taskist::store::Store;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let mut store = Store::open(&dir.path().join("new").join("tk.db"))?;
    /// let project = store.write(|tx| tx.insert_project("web", None, ""))?;
    /// let found = store.read(|tx| tx.project_by_name("web"))?;
    /// assert_eq!(found, Some(project));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn open(path: &Path) -> Result<Self, Error> {
        create_parent_dirs(path)?;
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        let version = user_version(&conn)?;
        refuse_newer(version)?;
        enable_wal(&conn)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        if version < SCHEMA_VERSION {
            migrate(&mut conn)?;
        }
        Ok(Self { conn })
    }

    /// Runs `work` in a deferred transaction, for commands that only read.
    pub fn read<T>(&mut self, work: impl FnOnce(&Tx<'_>) -> Result<T, Error>) -> Result<T, Error> {
        self.run(TransactionBehavior::Deferred, work)
    }

    /// Runs `work` in a `BEGIN IMMEDIATE` transaction, for commands that write.
    ///
    /// Nothing is kept when `work` fails: the transaction rolls back.
    pub fn write<T>(&mut self, work: impl FnOnce(&Tx<'_>) -> Result<T, Error>) -> Result<T, Error> {
        self.run(TransactionBehavior::Immediate, work)
    }

    fn run<T>(
        &mut self,
        behavior: TransactionBehavior,
        work: impl FnOnce(&Tx<'_>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let tx = Tx {
            tx: self.conn.transaction_with_behavior(behavior)?,
        };
        let value = work(&tx)?;
        tx.tx.commit()?;
        Ok(value)
    }
}

/// Creates the missing parent directories of `path`, with mode 0700 on Unix.
fn create_parent_dirs(path: &Path) -> Result<(), Error> {
    let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) else {
        return Ok(());
    };
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(parent).map_err(|err| {
        Error::Internal(format!(
            "cannot create the database directory {}: {err}",
            parent.display()
        ))
    })
}

/// Longest pause between two attempts of the WAL switch.
const WAL_RETRY_PAUSE_MAX: Duration = Duration::from_millis(50);

/// Switches the database to WAL mode.
///
/// On a database still in rollback mode, the switch upgrades the read lock the statement
/// holds to a write lock. When another process holds a read lock at that moment, waiting
/// could deadlock, so the engine reports `SQLITE_BUSY` at once without calling the busy
/// handler, and the statement releases its locks. The engine's resolution for that case is to
/// run the statement again, so the switch is repeated, with the busy handler's growing pauses, until
/// it succeeds or the busy timeout has passed.
fn enable_wal(conn: &Connection) -> Result<(), Error> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    let mut pause = Duration::from_millis(1);
    loop {
        match conn.query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        }) {
            Ok(mode) if mode.eq_ignore_ascii_case("wal") => return Ok(()),
            Ok(mode) => {
                return Err(Error::Internal(format!(
                    "cannot switch the database to WAL mode, it stays in {mode} mode"
                )));
            }
            Err(err) if is_busy(&err) && Instant::now() < deadline => {
                std::thread::sleep(pause);
                pause = (pause * 2).min(WAL_RETRY_PAUSE_MAX);
            }
            Err(err) => return Err(err.into()),
        }
    }
}

fn is_busy(err: &rusqlite::Error) -> bool {
    err.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)
}

fn user_version(conn: &Connection) -> Result<i64, Error> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

const fn refuse_newer(version: i64) -> Result<(), Error> {
    if version > SCHEMA_VERSION {
        return Err(Error::UnsupportedSchema {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(())
}

/// Applies every pending migration in one `BEGIN IMMEDIATE` transaction.
///
/// The version is read again after the lock is taken, so when several processes open a
/// new database at once, each migration runs exactly once.
fn migrate(conn: &mut Connection) -> Result<(), Error> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = user_version(&tx)?;
    refuse_newer(current)?;
    for (version, sql) in (1..).zip(MIGRATIONS) {
        if version > current {
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version)?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use rusqlite::Connection;
    use tempfile::TempDir;

    use super::{MIGRATIONS, NewTask, SCHEMA_VERSION, Store};
    use crate::error::Error;
    use crate::model::{NoteKind, Status};

    fn temp() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("taskist.db");
        (dir, path)
    }

    fn pragma(conn: &Connection, name: &str) -> String {
        conn.query_row(&format!("PRAGMA {name}"), [], |row| {
            row.get::<_, rusqlite::types::Value>(0)
        })
        .map(|value| match value {
            rusqlite::types::Value::Integer(n) => n.to_string(),
            rusqlite::types::Value::Text(text) => text,
            other => format!("{other:?}"),
        })
        .unwrap()
    }

    fn task(project_id: i64, title: &str) -> NewTask<'_> {
        NewTask {
            project_id,
            feature_id: None,
            title,
            body: "",
            priority: 2,
            created_by: "tester",
        }
    }

    #[test]
    fn migration_list_matches_the_schema_version() {
        assert_eq!(i64::try_from(MIGRATIONS.len()).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn pragma_reader_calibration() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(pragma(&conn, "user_version"), "0");
        assert_eq!(pragma(&conn, "journal_mode"), "memory");
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        assert_eq!(pragma(&conn, "foreign_keys"), "0");
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        assert_eq!(pragma(&conn, "foreign_keys"), "1");
    }

    #[test]
    fn the_wal_switch_waits_for_a_writer_that_blocks_the_lock_upgrade() {
        let (_dir, path) = temp();
        let writer = Connection::open(&path).unwrap();
        writer
            .execute_batch("CREATE TABLE t (x); BEGIN; INSERT INTO t VALUES (1);")
            .unwrap();
        let switcher = Connection::open(&path).unwrap();
        switcher.busy_timeout(super::BUSY_TIMEOUT).unwrap();

        // Calibration: while another connection holds a pending write, a plain switch fails
        // with SQLITE_BUSY at once, twice, although the busy timeout is set.
        for _ in 0..2 {
            let started = std::time::Instant::now();
            let plain = switcher.query_row("PRAGMA journal_mode = WAL", [], |row| {
                row.get::<_, String>(0)
            });
            assert!(plain.as_ref().is_err_and(super::is_busy), "{plain:?}");
            assert!(started.elapsed() < super::BUSY_TIMEOUT / 2);
        }

        std::thread::scope(|scope| {
            scope.spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(200));
                writer.execute_batch("COMMIT").unwrap();
            });
            super::enable_wal(&switcher).unwrap();
        });
        assert_eq!(pragma(&switcher, "journal_mode"), "wal");
        assert_eq!(pragma(&switcher, "user_version"), "0");
        let rows: i64 = switcher
            .query_row("SELECT count(*) FROM t", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn opening_a_missing_database_creates_directories_and_the_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("taskist.db");
        let store = Store::open(&path).unwrap();
        assert!(path.is_file());
        assert_eq!(pragma(&store.conn, "user_version"), "1");
        assert_eq!(pragma(&store.conn, "journal_mode"), "wal");
        assert_eq!(pragma(&store.conn, "foreign_keys"), "1");
        assert_eq!(pragma(&store.conn, "synchronous"), "1");
        assert_eq!(pragma(&store.conn, "busy_timeout"), "5000");
        #[cfg(unix)]
        assert_created_directories_are_private(dir.path());

        // A umask of 077 alone yields 0700, so the mode is also checked in a child run of
        // this test binary under umask 022, where a directory created without a mode gets
        // 0755; the child reports such a control directory to show the umask took effect.
        #[cfg(target_os = "linux")]
        {
            let output = std::process::Command::new("/usr/bin/sh")
                .arg("-c")
                .arg(r#"umask 022 && exec "$0" --exact "$1" --nocapture"#)
                .arg(format!("/proc/{}/exe", std::process::id()))
                .arg("store::tests::created_directories_are_private")
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(output.status.success(), "{output:?}");
            assert!(stdout.contains("test result: ok. 1 passed"), "{stdout}");
            assert!(stdout.contains("control directory mode 755"), "{stdout}");
        }
    }

    /// Checks that the directories `a` and `a/b` below `root` have mode 0700.
    #[cfg(unix)]
    fn assert_created_directories_are_private(root: &Path) {
        use std::os::unix::fs::PermissionsExt;
        for created in [root.join("a"), root.join("a").join("b")] {
            let mode = std::fs::metadata(&created).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "{}", created.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn created_directories_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        Store::open(&dir.path().join("a").join("b").join("taskist.db")).unwrap();
        assert_created_directories_are_private(dir.path());
        let control = dir.path().join("control");
        std::fs::create_dir(&control).unwrap();
        let mode = std::fs::metadata(&control).unwrap().permissions().mode() & 0o777;
        println!("control directory mode {mode:o}");
    }

    #[test]
    fn a_bare_file_name_needs_no_directory() {
        assert!(super::create_parent_dirs(Path::new("taskist.db")).is_ok());
    }

    #[test]
    fn a_parent_that_is_a_file_is_an_internal_error() {
        let (_dir, path) = temp();
        std::fs::write(&path, "").unwrap();
        let err = Store::open(&path.join("taskist.db")).unwrap_err();
        assert!(matches!(err, Error::Internal(_)), "{err:?}");
    }

    #[test]
    fn reopening_changes_nothing() {
        let (_dir, path) = temp();
        drop(Store::open(&path).unwrap());
        let before = std::fs::read(&path).unwrap();
        let store = Store::open(&path).unwrap();
        assert_eq!(pragma(&store.conn, "user_version"), "1");
        assert_eq!(pragma(&store.conn, "journal_mode"), "wal");
        assert_eq!(pragma(&store.conn, "foreign_keys"), "1");
        drop(store);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_newer_schema_is_refused_and_left_unchanged() {
        let (dir, path) = temp();
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE later (x TEXT); INSERT INTO later VALUES ('kept');
                 PRAGMA user_version = 2;",
            )
            .unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let err = Store::open(&path).unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnsupportedSchema {
                    found: 2,
                    supported: 1
                }
            ),
            "{err:?}"
        );
        assert_eq!(err.code(), "unsupported_schema");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["taskist.db"]);
    }

    #[test]
    fn a_newer_schema_found_under_the_migration_lock_is_refused() {
        let (_dir, path) = temp();
        let mut conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();
        let err = super::migrate(&mut conn).unwrap_err();
        assert!(
            matches!(err, Error::UnsupportedSchema { found: 3, .. }),
            "{err:?}"
        );
    }

    #[test]
    fn task_ids_are_never_reused() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let last = store
            .write(|tx| {
                let project = tx.insert_project("old", None, "")?;
                tx.insert_task(&task(project.id, "one"))?;
                tx.insert_task(&task(project.id, "two"))?;
                let last = tx.insert_task(&task(project.id, "three"))?;
                tx.delete_project(project.id)?;
                Ok(last.id)
            })
            .unwrap();
        let next = store
            .write(|tx| {
                assert!(tx.task(last)?.is_none());
                let project = tx.insert_project("new", None, "")?;
                tx.insert_task(&task(project.id, "four"))
            })
            .unwrap();
        assert!(next.id > last, "{} after {last}", next.id);
    }

    #[test]
    fn uniqueness_and_checks_hold_in_the_database() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let (project, other) = store
            .write(|tx| {
                Ok((
                    tx.insert_project("web", Some("/src/web"), "")?,
                    tx.insert_project("api", None, "")?,
                ))
            })
            .unwrap();
        let tries: Vec<(&str, Result<(), Error>)> = vec![
            (
                "duplicate project name",
                store.write(|tx| tx.insert_project("web", None, "").map(drop)),
            ),
            (
                "duplicate project path",
                store.write(|tx| tx.insert_project("www", Some("/src/web"), "").map(drop)),
            ),
            (
                "duplicate feature",
                store.write(|tx| {
                    tx.insert_feature(project.id, "login")?;
                    tx.insert_feature(project.id, "login").map(drop)
                }),
            ),
        ];
        for (what, result) in tries {
            assert!(
                matches!(result, Err(Error::Conflict(_))),
                "{what}: {result:?}"
            );
        }
        let priority = store.write(|tx| {
            tx.insert_task(&NewTask {
                priority: 4,
                ..task(project.id, "urgent")
            })
            .map(drop)
        });
        assert!(matches!(priority, Err(Error::Usage(_))), "{priority:?}");
        let status = store.write(|tx| {
            let created = tx.insert_task(&task(project.id, "t"))?;
            tx.tx.execute(
                "UPDATE task SET status = 'later' WHERE id = ?1",
                [created.id],
            )?;
            Ok(())
        });
        assert!(matches!(status, Err(Error::Usage(_))), "{status:?}");
        let missing = store.write(|tx| tx.insert_task(&task(999, "orphan")).map(drop));
        assert!(matches!(missing, Err(Error::NotFound(_))), "{missing:?}");

        // The same feature name in another project and several projects without a path are fine.
        store
            .write(|tx| {
                tx.insert_feature(other.id, "login")?;
                tx.insert_project("cli", None, "")
            })
            .unwrap();
        assert_eq!(store.read(|tx| tx.projects(true)).unwrap().len(), 3);
    }

    #[test]
    fn a_failure_in_the_middle_of_a_write_keeps_nothing() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let result = store.write(|tx| {
            let project = tx.insert_project("first", None, "")?;
            tx.insert_task(&NewTask {
                priority: 9,
                ..task(project.id, "second statement fails")
            })
        });
        assert!(matches!(result, Err(Error::Usage(_))), "{result:?}");
        assert!(store.read(|tx| tx.projects(true)).unwrap().is_empty());
        assert!(
            store
                .read(|tx| tx.project_by_name("first"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn writes_take_the_write_lock_at_begin_and_reads_do_not() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let other = Connection::open(&path).unwrap();
        other.busy_timeout(std::time::Duration::ZERO).unwrap();
        let try_write_lock = || {
            let taken = other.execute_batch("BEGIN IMMEDIATE");
            if taken.is_ok() {
                other.execute_batch("ROLLBACK").unwrap();
            }
            taken.is_ok()
        };
        assert!(
            try_write_lock(),
            "calibration: the lock is free outside transactions"
        );
        assert!(!store.write(|_| Ok(try_write_lock())).unwrap());
        assert!(store.read(|_| Ok(try_write_lock())).unwrap());
    }

    #[test]
    fn text_with_sql_syntax_is_stored_verbatim() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let hostile = "Robert'); DROP TABLE task; -- \"x\" ;";
        let (project, created, note) = store
            .write(|tx| {
                let project = tx.insert_project(hostile, Some(hostile), hostile)?;
                let feature = tx.insert_feature(project.id, hostile)?;
                let created = tx.insert_task(&NewTask {
                    feature_id: Some(feature.id),
                    body: hostile,
                    created_by: hostile,
                    ..task(project.id, hostile)
                })?;
                tx.add_tag(created.id, hostile)?;
                let note = tx.insert_note(created.id, NoteKind::Note, hostile, hostile)?;
                Ok((project, created, note))
            })
            .unwrap();
        let (read_project, read_task, notes) = store
            .read(|tx| {
                Ok((
                    tx.project_by_name(hostile)?,
                    tx.task(created.id)?,
                    tx.notes(created.id)?,
                ))
            })
            .unwrap();
        let read_project = read_project.unwrap();
        assert_eq!(read_project, project);
        assert_eq!(read_project.name, hostile);
        assert_eq!(read_project.path.as_deref(), Some(hostile));
        assert_eq!(read_project.description, hostile);
        let read_task = read_task.unwrap();
        assert_eq!(read_task.title, hostile);
        assert_eq!(read_task.body, hostile);
        assert_eq!(read_task.created_by, hostile);
        assert_eq!(read_task.tags, [hostile]);
        assert_eq!(notes, [note]);
        assert_eq!(notes[0].text, hostile);
        assert_eq!(
            store
                .read(|tx| tx.feature_by_name(project.id, hostile))
                .unwrap()
                .unwrap()
                .name,
            hostile
        );
    }

    #[test]
    fn projects_are_listed_by_name_and_archiving_hides_them() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let web = store
            .write(|tx| {
                tx.insert_project("zeta", None, "")?;
                let web = tx.insert_project("web", Some("/src/web"), "site")?;
                tx.insert_project("alpha", None, "")?;
                Ok(web)
            })
            .unwrap();
        assert!(!web.archived);
        assert!(
            web.created_at.ends_with('Z') && web.created_at.len() == 24,
            "{}",
            web.created_at
        );
        let names = |store: &mut Store, all| {
            store
                .read(|tx| tx.projects(all))
                .unwrap()
                .into_iter()
                .map(|project| project.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&mut store, false), ["alpha", "web", "zeta"]);
        store
            .write(|tx| tx.set_project_archived(web.id, true))
            .unwrap();
        assert_eq!(names(&mut store, false), ["alpha", "zeta"]);
        assert_eq!(names(&mut store, true), ["alpha", "web", "zeta"]);
        let found = store
            .read(|tx| tx.project_by_path("/src/web"))
            .unwrap()
            .unwrap();
        assert!(found.archived);
        assert!(
            store
                .read(|tx| tx.project_by_path("/nowhere"))
                .unwrap()
                .is_none()
        );
        let missing = store.write(|tx| tx.set_project_archived(999, true));
        assert!(matches!(missing, Err(Error::NotFound(_))), "{missing:?}");
        let missing = store.write(|tx| tx.delete_project(999));
        assert!(matches!(missing, Err(Error::NotFound(_))), "{missing:?}");
    }

    #[test]
    fn features_are_listed_by_name_within_their_project() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let (web, api) = store
            .write(|tx| {
                let web = tx.insert_project("web", None, "")?;
                let api = tx.insert_project("api", None, "")?;
                tx.insert_feature(web.id, "search")?;
                tx.insert_feature(web.id, "login")?;
                tx.insert_feature(api.id, "auth")?;
                Ok((web, api))
            })
            .unwrap();
        let names: Vec<String> = store
            .read(|tx| tx.features(web.id))
            .unwrap()
            .into_iter()
            .map(|feature| feature.name)
            .collect();
        assert_eq!(names, ["login", "search"]);
        assert!(
            store
                .read(|tx| tx.feature_by_name(api.id, "login"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_new_task_starts_as_todo_with_equal_timestamps() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let created = store
            .write(|tx| {
                let project = tx.insert_project("web", None, "")?;
                tx.insert_task(&task(project.id, "write docs"))
            })
            .unwrap();
        assert_eq!(created.status, Status::Todo);
        assert_eq!(created.priority, 2);
        assert_eq!(created.created_at, created.updated_at);
        assert!(created.closed_at.is_none());
        assert!(created.tags.is_empty());
        assert_eq!(store.read(|tx| tx.task(created.id)).unwrap(), Some(created));
    }

    #[test]
    fn status_changes_set_and_clear_closed_at() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let id = store
            .write(|tx| {
                let project = tx.insert_project("web", None, "")?;
                Ok(tx.insert_task(&task(project.id, "t"))?.id)
            })
            .unwrap();
        let after = |store: &mut Store, status| {
            store
                .write(|tx| {
                    tx.set_status(id, status)?;
                    tx.task(id)
                })
                .unwrap()
                .unwrap()
        };
        let doing = after(&mut store, Status::Doing);
        assert_eq!(doing.status, Status::Doing);
        assert!(doing.closed_at.is_none());
        let done = after(&mut store, Status::Done);
        assert!(done.closed_at.is_some());
        assert!(done.updated_at >= doing.updated_at);
        let reopened = after(&mut store, Status::Todo);
        assert!(reopened.closed_at.is_none());
        let dropped = after(&mut store, Status::Dropped);
        assert!(dropped.closed_at.is_some());
        let missing = store.write(|tx| tx.set_status(999, Status::Done));
        assert!(matches!(missing, Err(Error::NotFound(_))), "{missing:?}");
    }

    #[test]
    fn tasks_of_a_project_are_listed_by_id_with_their_tags() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let (web, first) = store
            .write(|tx| {
                let web = tx.insert_project("web", None, "")?;
                let api = tx.insert_project("api", None, "")?;
                let first = tx.insert_task(&task(web.id, "first"))?;
                tx.insert_task(&task(api.id, "elsewhere"))?;
                tx.insert_task(&task(web.id, "second"))?;
                tx.add_tag(first.id, "ui")?;
                tx.add_tag(first.id, "bug")?;
                tx.add_tag(first.id, "ui")?;
                Ok((web, first))
            })
            .unwrap();
        let tasks = store.read(|tx| tx.tasks(web.id)).unwrap();
        let titles: Vec<&str> = tasks.iter().map(|task| task.title.as_str()).collect();
        assert_eq!(titles, ["first", "second"]);
        assert_eq!(tasks[0].tags, ["bug", "ui"]);
        store.write(|tx| tx.remove_tag(first.id, "bug")).unwrap();
        assert_eq!(store.read(|tx| tx.tags(first.id)).unwrap(), ["ui"]);
        let missing = store.write(|tx| tx.add_tag(999, "x"));
        assert!(matches!(missing, Err(Error::NotFound(_))), "{missing:?}");
    }

    #[test]
    fn notes_are_listed_in_order_and_removed_only_with_their_project() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let (project, id) = store
            .write(|tx| {
                let project = tx.insert_project("web", None, "")?;
                let feature = tx.insert_feature(project.id, "login")?;
                let created = tx.insert_task(&NewTask {
                    feature_id: Some(feature.id),
                    ..task(project.id, "t")
                })?;
                tx.insert_note(created.id, NoteKind::Blocked, "waiting on api", "ann")?;
                tx.insert_note(created.id, NoteKind::Note, "second", "bob")?;
                tx.add_tag(created.id, "ui")?;
                Ok((project, created.id))
            })
            .unwrap();
        let notes = store.read(|tx| tx.notes(id)).unwrap();
        let kinds: Vec<NoteKind> = notes.iter().map(|note| note.kind).collect();
        assert_eq!(kinds, [NoteKind::Blocked, NoteKind::Note]);
        assert_eq!(notes[0].author, "ann");
        assert_eq!(notes[0].task_id, id);

        store.write(|tx| tx.delete_project(project.id)).unwrap();
        let count = |table: &str| -> i64 {
            store
                .conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap()
        };
        for table in ["project", "feature", "task", "task_tag", "note"] {
            assert_eq!(count(table), 0, "{table}");
        }
    }

    #[test]
    fn a_task_update_changes_its_fields_and_lookups_by_id_find_rows() {
        let (_dir, path) = temp();
        let mut store = Store::open(&path).unwrap();
        let (web, api, feature, id) = store
            .write(|tx| {
                let web = tx.insert_project("web", None, "")?;
                let api = tx.insert_project("api", None, "")?;
                let feature = tx.insert_feature(api.id, "db")?;
                let id = tx.insert_task(&task(web.id, "before"))?.id;
                Ok((web, api, feature, id))
            })
            .unwrap();
        assert_eq!(
            store.read(|tx| tx.project_by_id(web.id)).unwrap(),
            Some(web)
        );
        assert_eq!(
            store.read(|tx| tx.feature_by_id(feature.id)).unwrap(),
            Some(feature.clone())
        );
        assert!(store.read(|tx| tx.project_by_id(999)).unwrap().is_none());
        assert!(store.read(|tx| tx.feature_by_id(999)).unwrap().is_none());

        let update = super::TaskUpdate {
            project_id: api.id,
            feature_id: Some(feature.id),
            title: "after",
            body: "text",
            priority: 0,
        };
        let updated = store
            .write(|tx| {
                tx.update_task(id, &update)?;
                tx.task(id)
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                updated.project_id,
                updated.feature_id,
                updated.title.as_str(),
                updated.body.as_str(),
                updated.priority
            ),
            (api.id, Some(feature.id), "after", "text", 0)
        );
        assert!(updated.updated_at >= updated.created_at);
        let missing = store.write(|tx| tx.update_task(999, &update));
        assert!(matches!(missing, Err(Error::NotFound(_))), "{missing:?}");
    }
}
