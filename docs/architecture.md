# Taskist Architecture

> Status: MVP built per `docs/plan/20260927-1154-taskist-mvp.md`.

Taskist (`tk`) is a single-binary Rust CLI that stores tasks for many projects in one
central SQLite database (`rusqlite`, bundled). Tasks are grouped as
project -> feature -> task. One package, `taskist`, has a library target that holds every
behaviour and a thin `tk` binary target, so integration tests drive the binary and doctests
and in-process tests drive the library. All code is synchronous.

## Modules

```text
src/main.rs               captures arguments, environment variables, the current-directory
                          reader and the terminal state into an Env, calls taskist::run
src/lib.rs                run: parses arguments, selects the output format, dispatches each
                          command and returns its exit code; fail: reports a failure found
                          before a command ran
src/cli.rs                clap definitions of every command and option
src/env.rs                Env: the captured environment, database location, actor, colour rules
src/error.rs              Error: the error enumeration, its codes and exit codes, and the
                          mapping of clap and SQLite errors
src/scope.rs              project resolution: flag > TASKIST_PROJECT > current directory > none
src/model.rs              domain types, name and value validation, the status transition table
src/store.rs              connection setup (busy timeout, WAL, foreign keys), numbered
                          migrations tracked by user_version, read and write transactions
src/store/query.rs        bound-parameter queries for projects, features, tasks, tags, notes
src/store/migrations/     the embedded SQL migrations
src/command.rs            shared views of projects, features and tasks
src/command/task.rs       add, ls, show, edit, next, find
src/command/transition.rs start, block, done, drop, reopen, note
src/command/project.rs    project add, ls, show, edit, archive, rm
src/command/feature.rs    feature ls, mv
src/command/exchange.rs   brief, export, import
src/guide.rs              guide (embedded from docs/guide.md at build time), completions
src/output.rs             human text and JSON envelopes; the only module that writes to
                          stdout or stderr
```

Each command function opens the store, runs one transaction and returns a serializable
value; `output::emit` renders it as human text or as the JSON success envelope, or renders
the error. The library reads the process environment only through `Env`, so tests build one
by hand.

Integration tests live in `tests/`; `tests/common/mod.rs` runs every command in an empty
environment inside a temporary directory, so no test reaches a real database.

## MVP acceptance map

Each MVP acceptance criterion of section 7 of the plan, with tests that show it. Test names
are `path::function`.

| # | Criterion | Tests |
|---|---|---|
| 1 | All commands except `import` and `export md` work with human and `--json` output. | `tests/task.rs::add_creates_a_todo_task_in_the_task_shape`, `tests/task.rs::ls_is_a_table_ordered_by_project_and_feature_in_the_documented_layout`, `tests/task.rs::show_prints_the_task_and_its_notes_in_order`, `tests/task.rs::edit_changes_each_field_alone`, `tests/task.rs::next_orders_by_priority_then_doing_then_age_then_id`, `tests/task.rs::find_matches_title_body_and_notes_case_insensitively`, `tests/transition.rs::json_data_has_exactly_the_documented_fields`, `tests/transition.rs::human_output_names_each_task_and_whether_it_changed`, `tests/project.rs::show_reports_features_and_per_status_counts`, `tests/project.rs::rm_refuses_a_project_with_tasks_unless_forced`, `tests/feature.rs::mv_merges_into_an_existing_feature`, `tests/exchange.rs::brief_lists_open_tasks_by_feature_and_flags_blocked_ones`, `tests/exchange.rs::export_json_contains_every_project_feature_task_tag_and_note`, `tests/guide.rs::guide_json_returns_the_guide_in_data_text`, `tests/guide.rs::completions_bash_prints_a_script_for_tk` |
| 2 | Scope resolution works via flag, env var and directory mapping, with tests for precedence. | `tests/scope.rs::flag_wins_then_the_variable_then_the_directory_then_none`, `tests/scope.rs::the_longest_nested_project_path_wins`, `tests/scope.rs::an_archived_project_never_matches_by_directory`, `tests/scope.rs::all_projects_ignores_the_variable_and_the_directory`, `tests/task.rs::add_takes_the_project_from_the_variable_and_the_directory` |
| 3 | Invalid transitions and unknown ids return the documented exit codes and JSON errors. | `tests/transition.rs::an_invalid_transition_exits_4_and_changes_nothing`, `tests/transition.rs::an_unknown_id_is_not_found_for_every_command`, `tests/task.rs::show_of_an_unknown_id_is_not_found`, `src/model.rs::transition_table_matches_the_design_for_every_pair`, `src/error.rs::every_variant_maps_to_its_documented_code_and_exit_code` |
| 4 | Two concurrent writers complete without lost writes or lock errors. | `tests/concurrency.rs::concurrent_writer_processes_lose_no_task_and_report_no_lock_error`, `tests/store.rs::concurrent_first_opens_all_succeed_and_create_the_schema_once` |
| 5 | `tk guide` output alone is enough for an agent to add, list, update and close tasks. | `tests/guide.rs::the_guide_alone_operates_the_tool`, `tests/guide.rs::the_guide_names_every_subcommand_variable_error_code_and_exit_code` |
| 6 | Unit and integration tests with at least 80% line coverage; lint clean. | Lint: `tests/policy.rs::rust_lint_table_matches_policy`, `tests/policy.rs::clippy_lint_table_matches_policy`, `tests/policy.rs::both_crate_roots_forbid_unsafe_code`. Coverage: the `cargo llvm-cov nextest` measurement over the whole package, whose figure is recorded in `docs/changelog.md`. |

Line coverage is measured with `cargo llvm-cov nextest` over the whole package; the figure
of the MVP build is recorded in `docs/changelog.md`. `tests/common/mod.rs` passes
`LLVM_PROFILE_FILE` through to `tk`, so the processes the integration tests start count
towards it.

## Delivery

- `.github/workflows/ci.yml` runs on every push to `main` and every pull request: the quality
  gates (fmt, clippy, nextest, doctests, cargo-deny, cargo-shear, typos, the MSRV check) and a
  static musl build for `x86_64` and `aarch64`, each on a native runner.
- `.github/workflows/release.yml` runs on a `v*` tag: it checks that the tag matches the package
  version, builds `tk` with the `dist` profile for `x86_64-unknown-linux-musl` and
  `aarch64-unknown-linux-musl` on native runners, and publishes the binaries themselves as
  `tk-<target>` with a `SHA256SUMS` file as a GitHub release. A tag with a `-` suffix is
  published as a pre-release.
- `.github/scripts/build-static.sh` is the build both workflows share: `musl-gcc` compiles the
  bundled SQLite, the binary must have no program interpreter and no `NEEDED` entry, and it
  must run `tk --version` on the runner.
