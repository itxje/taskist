# Changelog

## 2026-09-27 11:54 [progress]

Initialized PMA docs. Drafted the MVP plan `20260927-1154-taskist-mvp` (project -> feature -> task model, SQLite store, agent-friendly CLI contract); awaiting approval.

## 2026-09-27 12:05 [decision]

Plan `20260927-1154-taskist-mvp`: switched implementation language from Go to Rust at the user's request. Storage uses `rusqlite` (bundled, synchronous); the `libsqlite3-sys` C dependency will be recorded as a baseline exception in `docs/decisions/`.

## 2026-09-27 12:10 [decision]

Plan `20260927-1154-taskist-mvp` approved: binary `tk`, five-status set, explicit project creation.

## 2026-09-27 12:30 [progress]

Task `20260927-1230-repository-foundation` completed: single `taskist` package with the `tk` binary, toolchain, lint, deny, fmt, clippy and nextest configuration, repository hygiene files and the bundled-SQLite decision record; `error`, `env`, `output` and `cli` modules behind a thin `main`; a shared integration-test helper that runs commands in an empty environment inside a temporary directory. Dependencies enter `Cargo.toml` with the change that first uses them: `rusqlite` with the SQLite store, `anstream` and `anstyle` with colour output, `clap_complete_command` with shell completions. The check that `tk` itself runs confined by the test helper follows with the SQLite store, when a command first opens the database.

## 2026-09-27 13:00 [progress]

Task `20260927-1300-sqlite-store` completed: `store` module (connection setup with busy timeout, WAL, foreign keys and `synchronous = NORMAL`; numbered migrations tracked by `user_version` and applied under `BEGIN IMMEDIATE`; refusal of a newer schema; write and read transactions; bound-parameter queries for projects, features, tasks, tags and notes) and `model` module (domain types, name and title validation, the status transition table). `rusqlite` 0.40.2 with the `bundled` feature enters `Cargo.toml` with its justification comment. `tk project ls` opens the database through the production path. The test helper is shown to confine `tk` itself, also when the caller's environment sets the variables `tk` reads.

## 2026-09-27 13:44 [progress]

Task `20260927-1344-scope-projects-features` completed: `scope` module (order `-p/--project`, `TASKIST_PROJECT`, current directory, none; component-wise longest-prefix matching on canonical paths; archived projects never match by directory; `--all-projects` for listing commands), global `--json` and `--by` options, actor resolution, the `project add|ls|show|edit|archive|rm` and `feature ls|mv` commands, and `tk ls` listing the open tasks of the resolved scope. `project ls --json` now returns `{"projects": [...]}`. The check that reads the stored actor through the binary follows with task creation.

## 2026-09-27 14:09 [progress]

Task `20260927-1409-task-commands` completed: `add`, `ls`, `show`, `edit`, `next` and `find` with human output and the JSON `data` shapes of the output contract. `add` stores `created_by` from `--by`, `TASKIST_ACTOR`, `USER` or `unknown`. The current directory is read only when a command needs directory scope, so `--help`, `--version` and commands scoped by `-p`, `TASKIST_PROJECT` or `--all-projects` work in an unreadable directory. Human output colours the status word and headings through `anstream`; stored text is printed as stored. `anstream` and `anstyle` enter the dependency set; `anyhow` leaves it, because `main` no longer has a fallible step.

## 2026-09-27 15:30 [progress]

Task `20260927-1530-task-command-fixes` completed: `find` folds case per character to a stable form, so texts that differ only in letter case match (word-final sigma and the capital sharp s included); `--pri` and `--limit` take values beginning with `-` and report them as bad values; invalid feature and tag names given to `ls` and `next` filters are usage errors; the colour decision follows the automatic rules of `anstream` (`NO_COLOR`, `CLICOLOR_FORCE`, `CLICOLOR`, `TERM`, `CI`) applied to the captured environment; an integration test pins age between priority and id in the `ls` and `find` order.

## 2026-09-27 16:20 [progress]

Task `20260927-1620-status-transitions` completed: `start`, `block`, `done`, `drop`, `reopen` and `note` on top of the single transition table, each in one immediate transaction; several ids are all-or-nothing, a no-op writes nothing, reasons and notes are stored with the resolved actor, and `closed_at` is set by `done` and `drop` and cleared by `reopen`. `done` takes leading plain-digit arguments as ids and at most one note. Project and feature names given to look something up are validated before the database is opened, so an invalid name is a `usage` error and a valid unknown one `not_found`.

## 2026-09-27 17:00 [progress]

Task `20260927-1700-brief-export-import` completed: `brief` prints a markdown digest of open work per project and feature with a separate list of blocked tasks and their reasons; `export` writes every project, feature, task (closed ones included), tag and note to stdout as a versioned JSON document or as markdown, restricted only by `-p`; `import` creates one task per JSON line from a file or stdin, validates every line before opening the database, and applies the whole file in one transaction, naming the line of the first error.
