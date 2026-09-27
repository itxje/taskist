CREATE TABLE project (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    path        TEXT UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    archived    INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    created_at  TEXT NOT NULL
);

CREATE TABLE feature (
    id         INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES project (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (project_id, name)
);

CREATE TABLE task (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES project (id) ON DELETE CASCADE,
    feature_id INTEGER REFERENCES feature (id) ON DELETE SET NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    status     TEXT NOT NULL
               CHECK (status IN ('todo', 'doing', 'blocked', 'done', 'dropped')),
    priority   INTEGER NOT NULL DEFAULT 2 CHECK (priority BETWEEN 0 AND 3),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    closed_at  TEXT,
    created_by TEXT NOT NULL
);

CREATE TABLE task_tag (
    task_id INTEGER NOT NULL REFERENCES task (id) ON DELETE CASCADE,
    tag     TEXT NOT NULL,
    PRIMARY KEY (task_id, tag)
);

CREATE TABLE note (
    id         INTEGER PRIMARY KEY,
    task_id    INTEGER NOT NULL REFERENCES task (id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('note', 'blocked', 'done', 'dropped')),
    text       TEXT NOT NULL,
    author     TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX task_project_status ON task (project_id, status);
CREATE INDEX task_feature ON task (feature_id);
CREATE INDEX note_task ON note (task_id);
