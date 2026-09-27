# 20260927-1344-scope-projects-features Add scope resolution and the project and feature commands

- **status**: in progress
- **priority**: P1
- **owner**: (unassigned)
- **createdAt**: 2026-09-27 13:44

## Description

Add the `scope` module, the global options `--json` and `--by`, actor resolution,
`--all-projects` for listing commands, the `project add|ls|show|edit|archive|rm` and
`feature ls|mv` commands with human and JSON output, and `tk ls --json` returning
`{scope, tasks}`. Part of plan `docs/plan/20260927-1154-taskist-mvp.md` (sections 3.6-3.9).

Acceptance:

- Precedence flag, `TASKIST_PROJECT`, current directory, none, read from `tk ls --json`.
- Directory matching: longest nested path; a sibling sharing a name prefix does not match;
  a symlinked working directory resolves to the project registered by real path; archived
  projects never match.
- `--all-projects` gives `{project: null, source: "none"}` inside a registered directory with
  `TASKIST_PROJECT` set, on `ls` and `feature ls`; with `-p` it exits 2.
- `-p` or `TASKIST_PROJECT` naming an unknown project exits 3 with `not_found`.
- `project add`: duplicate name or path exits 4 `conflict`; invalid name exits 2; a `--path`
  that is not an existing directory exits 2.
- `project ls|show|edit|archive|rm` as in section 3.8, human and JSON.
- `feature ls` open and total counts; `feature mv` rename, merge with `moved`, unknown source
  exits 3.
- Actor precedence `--by`, `TASKIST_ACTOR`, `USER`, `unknown`: unit tests on hand-built `Env`
  values and one integration test reading the stored actor through the binary.

## ActiveForm

Adding scope resolution and the project and feature commands

## Dependencies

- **blocked by**: `20260927-1300-sqlite-store`
- **blocks**: (none)

## Notes

### Investigation

- The store already has project, feature and task queries, `set_project_archived` and
  `delete_project` (cascading to features, tasks, tags and notes). Missing: a project update,
  task counts, feature rename, task move between features and feature deletion.
- `tk project ls --json` printed a bare array; the plan's contract is `{projects: [...]}`,
  so the two existing assertions on that output change with this task.
- No command writes an actor yet: tasks and notes are created by the task commands, which
  are not built. The actor is therefore resolvable (`Env::actor`) but not yet stored by any
  command.

### Design

- `scope::resolve(tx, env, Request { project, all_projects })`. `--all-projects` with `-p`
  is a `usage` error raised by the scope layer, so every listing command that takes
  `ListScope` gets the same rule. A flag or non-empty `TASKIST_PROJECT` is looked up by name
  (archived projects included) and is `not_found` when absent. Otherwise the captured current
  directory is canonicalized and compared with `Path::starts_with`, which compares whole
  components, against the stored canonical paths of non-archived projects; the match with the
  most components wins.
- `Scope` serializes as `{project: <name or null>, source}`; `Scope::require` gives the
  `usage` error naming `-p/--project` for commands that need a project (`feature mv` now,
  `add` and `import` later).
- Project paths: `--path` is joined to the captured current directory, canonicalized (so a
  symlink stores its target) and must be a directory with a UTF-8 path; anything else is
  `usage`. Name and path duplicates are checked first for a readable `conflict` message; the
  `UNIQUE` constraints back the checks.
- `Env::actor(by)`: first non-empty of `--by`, `TASKIST_ACTOR`, `USER`, else `unknown`.
- Counts come from one `GROUP BY feature_id, status` query per project (`Tx::task_counts`);
  open/total, per-status and per-feature numbers are derived with `Status::is_open`, so the
  open set is defined only in the model.
- `data` shapes: `project add`/`edit` `{project}`; `project ls` `{projects}`; project object
  `{name, path, description, archived, created_at, open}`; `project show`
  `{project, features, counts}` with `features` items `{project, name, open, total}` and
  `counts` one number per status; `project archive` `{project, changed}`; `project rm`
  `{removed, tasks}`; `feature ls` `{scope, features}`; `feature mv`
  `{feature, merged, moved}`, where `feature` is the target with its counts after the move
  and `moved` is the number of tasks that were in the old feature (also for a rename).
  `feature mv` with the same old and new name is a `usage` error, since a merge into itself
  would delete the feature.
- `tk ls` lists the open tasks of the scope (every non-archived project without one) by
  priority, then creation time, then id, in the task shape of section 3.9. Filters, feature
  grouping and the other listing options belong to the task commands.
- Human output: `project ls` prints `name  (N open)` with `  archived` when archived;
  `feature ls` prints a project heading and `  name  O open / T total` lines; `ls` prints a
  `name  (N open)` heading per project and `  #id  Pn  status  title` lines.
- Integration tests seed tasks and features through the library `Store` (the sandbox
  database), because no command creates tasks yet; the commands under test read and change
  them only through `tk`.

### Results

- Failing first: with the tests written and the old code in place, `cargo nextest run
  --locked --no-tests=pass --no-fail-fast` reported `108 tests run: 80 passed, 28 failed`;
  for example `mv_uses_the_directory_scope` failed with
  `unrecognized subcommand 'add'` (exit 2), and the `project ls` store tests failed on the
  `{projects: []}` shape.
- One failure after the implementation: `ls_lists_the_open_tasks_of_the_scope` expected
  `doing    seeded` and got `doing  seeded`, because `Status`'s `Display` writes the name
  without applying the width; the human task line formats `Status::as_str()` instead.
- `cargo fmt --all --check && cargo clippy --all-targets --locked && cargo nextest run
  --locked --no-tests=pass`: passed, 115 tests run, 115 passed, 0 skipped.
- `cargo llvm-cov nextest --no-fail-fast`: 72.3% line coverage. As recorded for the store
  task, `tk` runs through the helper in a cleared environment, so the profiles of the binary
  runs are not collected; the command modules are exercised by the integration tests but show
  as uncovered in this figure.
- Open: the integration test that reads an actor stored through the binary needs a command
  that stores one (`tk add` or `tk note`); none exists in this change.
