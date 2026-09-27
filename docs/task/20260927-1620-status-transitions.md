# 20260927-1620-status-transitions Add the status transition and note commands

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

## ActiveForm

Adding the status transition and note commands

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
  `cargo nextest run --locked --no-tests=pass`: 171 passed, 0 skipped.
