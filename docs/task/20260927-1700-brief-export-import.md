# 20260927-1700-brief-export-import Add the brief, export and import commands

- **status**: completed
- **priority**: P1
- **owner**: (unassigned)
- **createdAt**: 2026-09-27 17:00

## Description

Add `brief [-p P] [--all-projects]`, `export [--format json|md] [-p P]` and
`import <file.jsonl|-> [-p P]`, following sections 3.7, 3.8 and 3.9 of plan
`docs/plan/20260927-1154-taskist-mvp.md`.

Acceptance:

- `brief` lists open tasks grouped by feature, flags blocked tasks with their reason, omits
  closed tasks, and with `--json` returns the same text in `data.text`.
- `export --format json` prints a document that parses as JSON, has `version` 1 and contains
  every project (archived included), feature, task (closed included), tag and note; `-p`
  restricts it to one project; `TASKIST_PROJECT` and the working directory do not.
- `export --format md` contains every task id and title under its project and feature
  headings; with `--json` it is returned in `data.text`.
- `import` of a valid file creates one task per non-blank line with the given fields
  (`title`, `project`, `feature`, `pri`, `tags`, `body`, `by`) and reports the ids in file
  order; `import -` reads stdin.
- `import` with a malformed line, an unknown field, a missing title or an unknown project
  exits 2, 2, 2 and 3, names the line number and creates no task.
- Import lines without `project` use the resolved scope and fail with exit 2 naming
  `-p/--project` when none resolves.
- Export and import write only to stdout and read only the named file or stdin.

## ActiveForm

Adding the brief, export and import commands

## Dependencies

- **blocked by**: `20260927-1620-status-transitions`
- **blocks**: (none)

## Notes

### Investigation

- `task::list` with the default filter already returns what a digest needs: the open tasks of
  the resolved scope in display order, the open count of every covered project and the
  latest `blocked` note of every blocked task.
- `add` held the creation steps (archived check, feature on first use, insert, tags) inline;
  import needs exactly the same steps per line.
- `Store::write` rolls back when its closure fails, so returning the first failing line
  keeps nothing of the earlier ones.
- Stored timestamps come from SQL `strftime`; nothing returned the current time on its own.

### Proposal

- New module `command::exchange` with `brief`, `export` and `import`; the three commands are
  one flattened `ExchangeCommand` enum, dispatched by its own function.
- `brief` renders markdown from `task::list`: `# name (N open)` per project, `## feature` per
  feature with `## (no feature)` last, `- #id P<n> status title` per task, then a `Blocked:`
  list with `- #id title: reason`, or `no reason recorded`. Projects without open tasks are
  left out unless scoped; an empty result is `no open tasks`. `data` is `{scope, text}`.
- `export` reads every project with `Tx::projects(true)`, or only the `-p` project, and never
  resolves the scope. The document is `{version: 1, exported_at, projects: [{name, path,
  description, archived, created_at, features: [{name, created_at}], tasks: [{...task
  fields, notes}]}]}`, tasks by id, features and projects by name; `exported_at` comes from
  the new `Tx::now`. Human output prints the indented document; `--json` puts it in `data`.
  `md` prints a `## project` section with its fields, `### feature` headings with
  `### (no feature)` last, and a checkbox item per task, checked when closed, with tags,
  times, body and notes as nested items; `--json` returns `{text}`.
- `import` reads the whole file or stdin, splits it on newlines and numbers lines from 1.
  Each non-blank line is strict JSON with unknown fields refused, then title, priority and
  names are validated, all before the store is opened. In one write transaction each line
  creates its task through `task::create`, shared with `add`; a line's `project` is looked
  up by name, and the scope is resolved only for the first line without one. Every error
  keeps its kind and gets the prefix `line N: `. A line's `by` wins over `--by`. `data` is
  `{ids}`.
- An unreadable import file is an `internal` error naming the path, the documented class of
  I/O failures; invalid UTF-8 on a line is a `usage` error naming the line.

### Results

- Failing first: all 12 tests of `tests/exchange.rs` failed before the commands existed, for
  example `export_json_contains_every_project_feature_task_tag_and_note` with
  `unrecognized subcommand 'export'` (exit 2 instead of 0).
- `tests/exchange.rs` compares the full `brief` text for one project, for every project
  (no scope, `--all-projects` with `TASKIST_PROJECT` set, and `TASKIST_PROJECT`), and for an
  empty scope; compares the JSON export, in text and `--json` output, with the seeded
  content read back through `project show`, `show` and the stored features; checks the
  flag-only restriction and the refused `--format xml`, `--all-projects`, unknown and invalid
  names; checks the markdown headings and the position of every task line; and runs
  `import` from a file and stdin with every field, blank and CRLF lines, the scope from the
  flag, the variable and the directory, and ten kinds of bad line, counting tasks afterwards.
- Unit tests cover the line prefix, which keeps every error kind, and the continuation
  indent of multi-line text.
- `cargo fmt --all --check`, `cargo clippy --all-targets --locked` and
  `cargo nextest run --locked --no-tests=pass`: 195 passed, 0 skipped.
