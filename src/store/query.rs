//! Queries for projects, features, tasks, tags and notes. Every value is a bound parameter.

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::{OptionalExtension, Row, params};

use super::Tx;
use crate::error::Error;
use crate::model::{Feature, Note, NoteKind, Project, Since, Status, Task};

/// The current time as `YYYY-MM-DDTHH:MM:SS.sssZ`, inside SQL text.
macro_rules! now {
    () => {
        "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"
    };
}

macro_rules! project_columns {
    () => {
        "id, name, path, description, archived, created_at"
    };
}

macro_rules! feature_columns {
    () => {
        "id, project_id, name, created_at"
    };
}

macro_rules! task_columns {
    () => {
        "id, project_id, feature_id, title, body, status, priority, created_at, updated_at, \
         closed_at, created_by"
    };
}

macro_rules! note_columns {
    () => {
        "id, task_id, kind, text, author, created_at"
    };
}

/// The values of a new task; it starts as `todo`.
#[derive(Debug, Clone, Copy)]
pub struct NewTask<'a> {
    /// Owning project.
    pub project_id: i64,
    /// Feature, if any.
    pub feature_id: Option<i64>,
    /// Title, already normalized.
    pub title: &'a str,
    /// Free text.
    pub body: &'a str,
    /// 0 (most urgent) to 3.
    pub priority: u8,
    /// Actor that creates the task.
    pub created_by: &'a str,
}

/// The editable values of an existing task.
#[derive(Debug, Clone, Copy)]
pub struct TaskUpdate<'a> {
    /// Owning project.
    pub project_id: i64,
    /// Feature, if any; it must belong to the project.
    pub feature_id: Option<i64>,
    /// Title, already normalized.
    pub title: &'a str,
    /// Free text.
    pub body: &'a str,
    /// 0 (most urgent) to 3.
    pub priority: u8,
}

impl ToSql for Status {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for Status {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|err: Error| FromSqlError::Other(Box::new(err)))
    }
}

impl ToSql for NoteKind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for NoteKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|err: Error| FromSqlError::Other(Box::new(err)))
    }
}

fn project_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        description: row.get(3)?,
        archived: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn feature_row(row: &Row<'_>) -> rusqlite::Result<Feature> {
    Ok(Feature {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        created_at: row.get(3)?,
    })
}

/// A task without its tags; [`Tx::with_tags`] adds them.
fn task_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        project_id: row.get(1)?,
        feature_id: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        status: row.get(5)?,
        priority: row.get(6)?,
        tags: Vec::new(),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        closed_at: row.get(9)?,
        created_by: row.get(10)?,
    })
}

fn note_row(row: &Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        task_id: row.get(1)?,
        kind: row.get(2)?,
        text: row.get(3)?,
        author: row.get(4)?,
        created_at: row.get(5)?,
    })
}

/// Fails with `not_found` when a statement meant to change one row changed none.
fn expect_one(changed: usize, what: &str, id: i64) -> Result<(), Error> {
    if changed == 0 {
        return Err(Error::NotFound(format!("no {what} with id {id}")));
    }
    Ok(())
}

impl Tx<'_> {
    /// The current time, in the format of every stored timestamp.
    pub fn now(&self) -> Result<String, Error> {
        Ok(self
            .tx
            .query_row(concat!("SELECT ", now!()), [], |row| row.get(0))?)
    }

    /// The earliest creation time `since` keeps, in the format of every stored timestamp,
    /// so it compares with them as text; `None` when `SQLite` cannot represent it or a date
    /// does not exist in the calendar. A date is read as local midnight through the
    /// `utc` modifier of `SQLite`, which follows the time zone of the process.
    pub fn cutoff(&self, since: &Since) -> Result<Option<String>, Error> {
        const AGO: &str = "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)";
        let (sql, value) = match since {
            Since::Minutes(amount) => (AGO, format!("-{amount} minutes")),
            Since::Hours(amount) => (AGO, format!("-{amount} hours")),
            Since::Days(amount) => (AGO, format!("-{amount} days")),
            Since::Date(date) => (
                "SELECT CASE WHEN date(?1) IS ?1 \
                 THEN strftime('%Y-%m-%dT%H:%M:%fZ', ?1, 'utc') END",
                date.clone(),
            ),
        };
        Ok(self.tx.query_row(sql, [value], |row| row.get(0))?)
    }

    /// Creates a project.
    pub fn insert_project(
        &self,
        name: &str,
        path: Option<&str>,
        description: &str,
    ) -> Result<Project, Error> {
        Ok(self.tx.query_row(
            concat!(
                "INSERT INTO project (name, path, description, created_at) VALUES (?1, ?2, ?3, ",
                now!(),
                ") RETURNING ",
                project_columns!()
            ),
            params![name, path, description],
            project_row,
        )?)
    }

    /// Projects ordered by name; archived ones only when `include_archived`.
    pub fn projects(&self, include_archived: bool) -> Result<Vec<Project>, Error> {
        let mut stmt = self.tx.prepare_cached(concat!(
            "SELECT ",
            project_columns!(),
            " FROM project WHERE ?1 OR archived = 0 ORDER BY name"
        ))?;
        let rows = stmt.query_map([include_archived], project_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The project with this name.
    pub fn project_by_name(&self, name: &str) -> Result<Option<Project>, Error> {
        Ok(self
            .tx
            .query_row(
                concat!(
                    "SELECT ",
                    project_columns!(),
                    " FROM project WHERE name = ?1"
                ),
                [name],
                project_row,
            )
            .optional()?)
    }

    /// The project with this id.
    pub fn project_by_id(&self, id: i64) -> Result<Option<Project>, Error> {
        Ok(self
            .tx
            .query_row(
                concat!("SELECT ", project_columns!(), " FROM project WHERE id = ?1"),
                [id],
                project_row,
            )
            .optional()?)
    }

    /// The project linked to this directory.
    pub fn project_by_path(&self, path: &str) -> Result<Option<Project>, Error> {
        Ok(self
            .tx
            .query_row(
                concat!(
                    "SELECT ",
                    project_columns!(),
                    " FROM project WHERE path = ?1"
                ),
                [path],
                project_row,
            )
            .optional()?)
    }

    /// Sets the name, path and description of a project.
    pub fn update_project(
        &self,
        id: i64,
        name: &str,
        path: Option<&str>,
        description: &str,
    ) -> Result<Project, Error> {
        self.tx
            .query_row(
                concat!(
                    "UPDATE project SET name = ?2, path = ?3, description = ?4 WHERE id = ?1 \
                     RETURNING ",
                    project_columns!()
                ),
                params![id, name, path, description],
                project_row,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("no project with id {id}")))
    }

    /// The number of tasks of a project per feature and status; `None` is the tasks
    /// without a feature. Combinations without tasks are absent.
    pub fn task_counts(&self, project_id: i64) -> Result<Vec<(Option<i64>, Status, i64)>, Error> {
        let mut stmt = self.tx.prepare_cached(
            "SELECT feature_id, status, count(*) FROM task WHERE project_id = ?1 \
             GROUP BY feature_id, status",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Archives or unarchives a project.
    pub fn set_project_archived(&self, id: i64, archived: bool) -> Result<(), Error> {
        let changed = self.tx.execute(
            "UPDATE project SET archived = ?2 WHERE id = ?1",
            params![id, archived],
        )?;
        expect_one(changed, "project", id)
    }

    /// Deletes a project with its features, tasks, tags and notes.
    pub fn delete_project(&self, id: i64) -> Result<(), Error> {
        let changed = self.tx.execute("DELETE FROM project WHERE id = ?1", [id])?;
        expect_one(changed, "project", id)
    }

    /// Creates a feature in a project.
    pub fn insert_feature(&self, project_id: i64, name: &str) -> Result<Feature, Error> {
        Ok(self.tx.query_row(
            concat!(
                "INSERT INTO feature (project_id, name, created_at) VALUES (?1, ?2, ",
                now!(),
                ") RETURNING ",
                feature_columns!()
            ),
            params![project_id, name],
            feature_row,
        )?)
    }

    /// The features of a project, ordered by name.
    pub fn features(&self, project_id: i64) -> Result<Vec<Feature>, Error> {
        let mut stmt = self.tx.prepare_cached(concat!(
            "SELECT ",
            feature_columns!(),
            " FROM feature WHERE project_id = ?1 ORDER BY name"
        ))?;
        let rows = stmt.query_map([project_id], feature_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The feature with this name in a project.
    pub fn feature_by_name(&self, project_id: i64, name: &str) -> Result<Option<Feature>, Error> {
        Ok(self
            .tx
            .query_row(
                concat!(
                    "SELECT ",
                    feature_columns!(),
                    " FROM feature WHERE project_id = ?1 AND name = ?2"
                ),
                params![project_id, name],
                feature_row,
            )
            .optional()?)
    }

    /// The feature with this id.
    pub fn feature_by_id(&self, id: i64) -> Result<Option<Feature>, Error> {
        Ok(self
            .tx
            .query_row(
                concat!("SELECT ", feature_columns!(), " FROM feature WHERE id = ?1"),
                [id],
                feature_row,
            )
            .optional()?)
    }

    /// Renames a feature.
    pub fn rename_feature(&self, id: i64, name: &str) -> Result<(), Error> {
        let changed = self.tx.execute(
            "UPDATE feature SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        expect_one(changed, "feature", id)
    }

    /// Moves every task of feature `from` to feature `to` and returns how many moved.
    pub fn move_feature_tasks(&self, from: i64, to: i64) -> Result<usize, Error> {
        Ok(self.tx.execute(
            concat!(
                "UPDATE task SET feature_id = ?2, updated_at = ",
                now!(),
                " WHERE feature_id = ?1"
            ),
            params![from, to],
        )?)
    }

    /// Deletes a feature; its tasks, if any, lose their feature.
    pub fn delete_feature(&self, id: i64) -> Result<(), Error> {
        let changed = self.tx.execute("DELETE FROM feature WHERE id = ?1", [id])?;
        expect_one(changed, "feature", id)
    }

    /// Creates a task in status `todo`.
    pub fn insert_task(&self, task: &NewTask<'_>) -> Result<Task, Error> {
        Ok(self.tx.query_row(
            concat!(
                "INSERT INTO task (project_id, feature_id, title, body, status, priority, \
                 created_at, updated_at, created_by) VALUES (?1, ?2, ?3, ?4, 'todo', ?5, ",
                now!(),
                ", ",
                now!(),
                ", ?6) RETURNING ",
                task_columns!()
            ),
            params![
                task.project_id,
                task.feature_id,
                task.title,
                task.body,
                task.priority,
                task.created_by
            ],
            task_row,
        )?)
    }

    /// The task with this id, with its tags.
    pub fn task(&self, id: i64) -> Result<Option<Task>, Error> {
        let task = self
            .tx
            .query_row(
                concat!("SELECT ", task_columns!(), " FROM task WHERE id = ?1"),
                [id],
                task_row,
            )
            .optional()?;
        task.map(|task| self.with_tags(task)).transpose()
    }

    /// The tasks of a project ordered by id, with their tags.
    pub fn tasks(&self, project_id: i64) -> Result<Vec<Task>, Error> {
        let mut stmt = self.tx.prepare_cached(concat!(
            "SELECT ",
            task_columns!(),
            " FROM task WHERE project_id = ?1 ORDER BY id"
        ))?;
        let rows = stmt.query_map([project_id], task_row)?;
        rows.map(|task| self.with_tags(task?)).collect()
    }

    /// Sets the project, feature, title, body and priority of a task and its update time.
    pub fn update_task(&self, id: i64, update: &TaskUpdate<'_>) -> Result<(), Error> {
        let changed = self.tx.execute(
            concat!(
                "UPDATE task SET project_id = ?2, feature_id = ?3, title = ?4, body = ?5, \
                 priority = ?6, updated_at = ",
                now!(),
                " WHERE id = ?1"
            ),
            params![
                id,
                update.project_id,
                update.feature_id,
                update.title,
                update.body,
                update.priority
            ],
        )?;
        expect_one(changed, "task", id)
    }

    /// Sets the status of a task and its update time; `closed_at` is set for `done` and
    /// `dropped` and cleared otherwise.
    pub fn set_status(&self, id: i64, status: Status) -> Result<(), Error> {
        let changed = self.tx.execute(
            concat!(
                "UPDATE task SET status = ?2, updated_at = ",
                now!(),
                ", closed_at = CASE WHEN ?3 THEN ",
                now!(),
                " END WHERE id = ?1"
            ),
            params![id, status, status.is_closed()],
        )?;
        expect_one(changed, "task", id)
    }

    /// Sets the update time of a task to now.
    pub fn touch_task(&self, id: i64) -> Result<(), Error> {
        let changed = self.tx.execute(
            concat!("UPDATE task SET updated_at = ", now!(), " WHERE id = ?1"),
            [id],
        )?;
        expect_one(changed, "task", id)
    }

    fn with_tags(&self, mut task: Task) -> Result<Task, Error> {
        task.tags = self.tags(task.id)?;
        Ok(task)
    }

    /// Adds a tag to a task; adding a tag it already has changes nothing.
    pub fn add_tag(&self, task_id: i64, tag: &str) -> Result<(), Error> {
        self.tx.execute(
            "INSERT INTO task_tag (task_id, tag) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
            params![task_id, tag],
        )?;
        Ok(())
    }

    /// Removes a tag from a task.
    pub fn remove_tag(&self, task_id: i64, tag: &str) -> Result<(), Error> {
        self.tx.execute(
            "DELETE FROM task_tag WHERE task_id = ?1 AND tag = ?2",
            params![task_id, tag],
        )?;
        Ok(())
    }

    /// The tags of a task, sorted.
    pub fn tags(&self, task_id: i64) -> Result<Vec<String>, Error> {
        let mut stmt = self
            .tx
            .prepare_cached("SELECT tag FROM task_tag WHERE task_id = ?1 ORDER BY tag")?;
        let rows = stmt.query_map([task_id], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Appends a note to a task.
    pub fn insert_note(
        &self,
        task_id: i64,
        kind: NoteKind,
        text: &str,
        author: &str,
    ) -> Result<Note, Error> {
        Ok(self.tx.query_row(
            concat!(
                "INSERT INTO note (task_id, kind, text, author, created_at) VALUES (?1, ?2, ?3, ?4, ",
                now!(),
                ") RETURNING ",
                note_columns!()
            ),
            params![task_id, kind, text, author],
            note_row,
        )?)
    }

    /// The notes of a task, oldest first.
    pub fn notes(&self, task_id: i64) -> Result<Vec<Note>, Error> {
        let mut stmt = self.tx.prepare_cached(concat!(
            "SELECT ",
            note_columns!(),
            " FROM note WHERE task_id = ?1 ORDER BY id"
        ))?;
        let rows = stmt.query_map([task_id], note_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
