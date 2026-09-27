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

## 2026-09-27 18:00 [progress]

Task `20260927-1800-exchange-fixes` completed: stored text written into markdown (`brief` blocked reasons, and the project description, task body and note text of `export --format md`) is broken at every line terminator and indented under its item, so it cannot form a heading or list item; an import error names only the line of the file, with the parser's position given as a column; `import` validates every line before the database is opened and resolves a project named by `-p` or `TASKIST_PROJECT` before any task is created, so an unknown one fails with `not_found` and creates nothing.

## 2026-09-27 19:00 [progress]

Task `20260927-1900-guide-docs-coverage` completed: `docs/guide.md` is embedded at build time and printed by `tk guide` (`data.text` with `--json`), and an integration test runs every line of its `sh` blocks against a temporary database; `tk completions <shell>` prints completion scripts through `clap_complete_command` 0.6.1; the README covers installation, quick start, environment variables, the command reference, the JSON contract, exit codes and backups as copies of the database file; `docs/architecture.md` lists the modules as built and maps each MVP acceptance criterion to its tests; a test runs two loops of 50 concurrent `tk add` processes; the public library entry points carry doctests. The test helper passes `LLVM_PROFILE_FILE` to `tk`, so the processes integration tests start count towards coverage. `cargo llvm-cov nextest` over the whole package reports 97.91% line coverage (3725 lines, 78 missed).

## 2026-09-27 20:00 [progress]

Task `20260927-2000-empty-values-coverage-map` completed: the guide and the README name which empty environment values count as unset (`TASKIST_DB`, `XDG_DATA_HOME`, `HOME`, `TASKIST_PROJECT`, `TASKIST_ACTOR`, `USER`, `NO_COLOR`, `CLICOLOR_FORCE`), which count as set (`CLICOLOR`, `TERM`, `CI`, following the colour rules of `anstream`), and that an empty `TASKIST_FORMAT` is a usage error; the acceptance map names the `cargo llvm-cov nextest` measurement and this changelog as the coverage evidence. Line coverage over the whole package: 97.92%.

## 2026-09-27 17:36 [decision]

Plan `20260927-1728-ci-and-release` approved: GitHub Actions CI runs the quality gates and a static musl build on native `x86_64` and `aarch64` runners; a `v*` tag builds `tk` for `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` with the `dist` profile (bundled SQLite compiled by `musl-gcc`) and publishes the archives with `SHA256SUMS` as a GitHub release. Actions are pinned to commit SHAs and CI tools to exact versions.

## 2026-09-27 17:38 [progress]

Task `20260927-1728-ci-and-release` completed: CI passes on `main` (quality gates, and static musl builds on native `x86_64` and `aarch64` runners that run `tk --version`). Release `v0.1.0` is published with `tk-0.1.0-x86_64-unknown-linux-musl.tar.gz`, `tk-0.1.0-aarch64-unknown-linux-musl.tar.gz` and `SHA256SUMS`; the downloaded aarch64 binary verifies against the checksums, is statically linked and runs.

## 2026-09-27 18:00 [progress]

Task `20260927-1800-release-raw-binaries` completed: a release publishes the static binaries themselves as `tk-x86_64-unknown-linux-musl` and `tk-aarch64-unknown-linux-musl` with `SHA256SUMS`, instead of `tar.gz` archives, so `releases/latest/download/tk-<target>` always names the newest build. Release `v0.1.0` now carries these assets (the binaries extracted unchanged from its archives, which were removed).

## 2026-09-27 20:10 [progress]

Task `20260927-1955-colour-priority-project` completed: human output styles the priority (`P0` bold red, `P1` magenta, `P2` plain, `P3` dimmed grey) in task lines and `tk show`, and project names in bold blue in `tk ls`, `tk find`, `tk next`, `tk show`, `tk project ls`, `tk project show` and `tk feature ls`, so projects stand apart from the plain bold feature headings. The colour rules are unchanged; uncoloured and JSON output are byte-for-byte the same as before.

## 2026-09-27 20:45 [progress]

Task `20260927-2005-table-task-list` completed: `tk ls` and `tk find` print a borderless table with the columns `ID`, `PRI`, `STATUS`, `PROJECT` (left out when the list is scoped to one project), `FEATURE`, `AGE` and `TITLE`, each padded to its widest cell outside any colour, so ids of different widths no longer shift the columns; the per-project `(N open)` headings are gone (`tk project ls` keeps the counts). `AGE` is the time since creation in whole minutes, hours or days. `tk ls --since WHEN` keeps tasks created in the last `<N>m`, `<N>h`, `<N>d` or `<N>w`, or since local midnight of a date `YYYY-MM-DD`; the cutoff is computed by SQLite, and malformed or unrepresentable values are `usage` errors. JSON output is unchanged apart from the new filter.
