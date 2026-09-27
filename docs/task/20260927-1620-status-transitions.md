# 20260927-1620-status-transitions Add the status transition and note commands and validate lookup names

- **status**: completed
- **priority**: P1
- **owner**: (unassigned)
- **createdAt**: 2026-09-27 16:20

## Description

Add `start <id>...`, `block <id> <reason>`, `done <id>... [note]` with `--note TEXT`,
`drop <id> <reason>`, `reopen <id>...` and `note <id> <text>` on top of the transition table in
`model`, following sections 3.8 and 3.9 of plan `docs/plan/20260927-1154-taskist-mvp.md`.

Acceptance:

- Through the binary: each valid change of the table, each no-op (exit 0, `changed` false, no
  new note, `updated_at` unchanged), each invalid transition (exit 4, `invalid_transition`) and
  an unknown id (exit 3, `not_found`).
- Several ids are all-or-nothing: `tk done <valid> <unknown>` exits 3 and `tk start <valid>
  <dropped>` exits 4, both leaving the valid task unchanged. `tk done 1 2 "text"` closes both
  and stores the note on both; `tk done 1 --note 42` stores the note `42`.
- `block` and `drop` store the reason as a note of kind `blocked` / `dropped` with the resolved
  actor as author; `closed_at` is set by `done` and `drop` and cleared by `reopen`.
- `note` appends a note of kind `note`, moves `updated_at`, and exits 3 for an unknown id.
- The JSON `data` is `{tasks: [{task, changed}]}` in argument order for transitions and
  `{task}` for `note`, with exactly those keys.
- An invalid project name (for example `BAD_NAME`) in `-p` of `ls`, `next`, `find`,
  `feature ls`, `feature mv`, `add` and `edit`, in `TASKIST_PROJECT`, and in
  `project show|edit|archive|rm`, and an invalid feature name as `feature mv <old>`, exits 2
  with `usage`; a valid unknown name exits 3 with `not_found`. The name is checked before
  the store is opened, so the answer is the same for an unknown task id, for a database that
  cannot be opened, and on a fresh environment, where no database is created.
- `done` takes as ids only leading arguments made of ASCII digits: `tk done 2 +1` stores the
  note `+1` on task 2 and leaves task 1 open; an all-digit argument too large for an id is a
  `usage` error naming it.

## ActiveForm

Adding the status transition and note commands and validating lookup names

## Dependencies

- **blocked by**: `20260927-1409-task-commands`
- **blocks**: (none)

## Notes

### Investigation

- `model::Action::apply` already holds the single transition table and its unit test covers
  every action and status pair; no command uses it yet.
- `Tx::set_status` sets `updated_at`, sets `closed_at` for `done` and `dropped` and clears it
  otherwise; `Tx::insert_note` appends a note. Nothing moves `updated_at` alone.
- `Store::write` runs one `BEGIN IMMEDIATE` transaction and rolls back when the closure fails,
  so returning the first error of a multi-id command keeps nothing of the earlier ids.
- The `ls` layout fixture seeds its blocked, doing and done tasks through the library, since no
  command changed a status before.

- Names given to look something up were not validated: `scope::named` (the `-p` flag and
  `TASKIST_PROJECT`) and `command::project_named` (`project show|edit|archive|rm`, `edit -p`)
  looked the name up directly, and `feature mv` validated only `<new>`, so an invalid name
  was reported as `not_found` instead of `usage`. Every other `project_by_name` and
  `feature_by_name` caller already receives a validated or stored name.

- A name check inside the lookup runs after the store is opened and, in `edit`, after the
  task is looked up, so an invalid `-p` name was reported as `not_found` for an unknown task
  and as `unsupported_schema` for a newer database, and a fresh environment got a database
  file before the refusal.
- `split_done_args` took every argument `i64::from_str` accepts as an id, including `+1`.

### Proposal

- The status commands are one flattened `StatusCommand` enum, dispatched by its own function,
  so the top-level dispatch stays short.
- New module `command::transition` with `transition` (shared by the five status commands),
  `note`, and `split_done_args`, which takes the leading integer arguments of `done` as ids and
  at most one following argument as the note; a positional note together with `--note`, an
  argument after the note, or no id at all is a `usage` error.
- Per id, in argument order, inside one write transaction: read the task (`not_found`), apply
  the action; `Invalid` fails the command with `invalid_transition`; `NoOp` writes nothing;
  `Change` sets the status and, for `block`, `drop` and `done` with a note, appends the note of
  the matching kind authored by the resolved actor. A no-op writes no row, so the transaction
  commits without changing the file.
- `Tx::touch_task` moves `updated_at` for `note`.
- Human text: `#<id> is now <status>: <title>` or `#<id> is already <status>: <title>` per task,
  `noted #<id> in <location>: <title>` for `note`.
- The `ls` layout fixture seeds its statuses and reasons through `tk start`, `tk block`,
  `tk note` and `tk done`.
- Validate every name where it enters, before the store is opened. `scope::target` turns the
  scope request (`-p`, `TASKIST_PROJECT`, `--all-projects`, the directory) into a `Target`
  and validates its project name without reading the database; `scope::resolve` accepts only
  a `Target`, so no scope name reaches a lookup unvalidated. `project show|edit|archive|rm`
  and `edit -p` validate their name at the top of the command; `command::project_named` only
  looks up. `feature mv` validates `<old>` next to `<new>`.
- An id of `done` is a non-empty run of ASCII digits that parses as an `i64`; a longer one is
  a `usage` error naming the value.

### Results

- Failing first: 12 of the 14 tests of `tests/transition.rs` failed before the commands
  existed, for example `done_takes_leading_integers_as_ids_and_one_note` with `unrecognized
  subcommand 'done'` (exit 2 instead of 0). The two that passed are the calibration test,
  which uses no new command, and `done_refuses_ambiguous_arguments_as_usage_errors`, whose
  expected exit 2 the unknown subcommand also produced.
- `tests/transition.rs` covers each change of the table (14), each no-op (5, comparing
  `tk show --json` and the database file before and after), each invalid transition (6), an
  unknown id for all six commands, all-or-nothing runs with several ids, the argument split of
  `done` including `--note 42`, reasons and notes with the flag, `TASKIST_ACTOR`, `USER` and
  `unknown` as author, `closed_at` across `done`, `drop` and `reopen`, `note`, the JSON key
  sets and the human text. A calibration test first checks that the seeded statuses and the
  aged `updated_at` read back as seeded.
- Unit tests cover `split_done_args` and `Action::target`.
- The `ls` layout fixture now reaches its statuses and reasons through `tk start`, `tk block`,
  `tk note` and `tk done`; its expected output is unchanged.
- `cargo fmt --all --check`, `cargo clippy --all-targets --locked` and
  `cargo nextest run --locked --no-tests=pass`: 175 passed, 0 skipped, after the first
  version of the name validation.
- Names before the store and digit ids, failing first: with the source before this change,
  `an_invalid_project_name_in_edit_is_a_usage_error_even_for_an_unknown_task` (exit 3 for
  `edit 999 -p BAD_NAME`), `an_invalid_name_is_a_usage_error_on_a_database_that_cannot_be_opened`
  (exit 1 `unsupported_schema`), `an_invalid_project_name_creates_no_database` and
  `done_takes_only_plain_digits_as_ids` (task 1 closed by the note `+1`) failed; their
  calibration cases passed. The unit tests `only_plain_digits_are_ids`
  (`left: ([2, 1], None)`, `right: ([2], Some("+1"))`) and
  `a_number_too_large_for_an_id_is_a_usage_error_naming_it` failed too. All pass after it.
- Every command that takes a name validates it before opening the store: `add -p/-f/--tag`,
  `edit -p/-f/--tag`, `ls -p/-f/--tag`, `next -p/-f`, `find -p`, `feature ls -p`,
  `feature mv <old> <new> -p`, `TASKIST_PROJECT` through `scope::target`, and
  `project add|show|edit|archive|rm` including `edit --name`.
- `cargo fmt --all --check`, `cargo clippy --all-targets --locked` and
  `cargo nextest run --locked --no-tests=pass`: 181 passed, 0 skipped. `typos`, `cargo shear`
  and `cargo deny check` report no issues.
- Name validation, first version, failing first: in `tests/names.rs` the three tests with an invalid name
  failed before the change with exit 3 where 2 was expected (`left: Some(3)`,
  `right: Some(2)`); the calibration test, which runs every command line with the known name
  `web`, passed. After the change all four pass. Each case also checks that a valid unknown
  name stays `not_found` and that the refused commands left the task unchanged.
