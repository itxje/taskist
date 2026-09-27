# 20260927-1900-guide-docs-coverage Add the guide, completions, documentation, concurrency test and coverage

- **status**: completed
- **priority**: P1
- **owner**: (unassigned)
- **createdAt**: 2026-09-27 19:00

## Description

Deliver W6 of plan `docs/plan/20260927-1154-taskist-mvp.md`: `docs/guide.md` embedded at
build time for `tk guide`, `tk completions <shell>`, the final `README.md`, the modules as
built and an acceptance map in `docs/architecture.md`, the concurrent-writers test, doctests
on the public library entry points, and at least 80% line coverage measured with
`cargo llvm-cov nextest` over the whole package.

Acceptance:

- An integration test reads `tk guide`, runs every line of its fenced `sh` blocks in order
  against a fresh temporary database, asserts each exits 0, and asserts the final state after
  adding projects and tasks, listing, editing and closing.
- An integration test compares the guide with the subcommand list of the clap definition,
  the `TASKIST_*` names in the sources and the other variables the tool reads, and the error
  enumeration with its exit codes.
- `tk guide --json` returns the guide in `data.text`.
- Two writer loops of 50 `tk add` processes each against one database: every command exits
  0, the database holds 100 tasks with 100 distinct ids, and no stderr line mentions a locked
  or busy database.
- `tk completions bash` prints a non-empty script that mentions `tk`; an unknown shell exits 2.
- Doctests exercise the public library entry points.
- Line coverage is at least 80% and recorded in `docs/changelog.md`.
- A test reads the acceptance map of `docs/architecture.md` and checks that every named test
  exists.

## ActiveForm

Adding the guide, completions, documentation, concurrency test and coverage

## Dependencies

- **blocked by**: `20260927-1800-exchange-fixes`
- **blocks**: (none)

## Notes

### Investigation

- `tests/common/mod.rs` clears the environment of every child, including
  `LLVM_PROFILE_FILE`. Under `cargo llvm-cov` an instrumented `tk` then writes its profile as
  `default_*.profraw` into the temporary working directory, where the coverage run never
  collects it, so every line only the binary runs counted as missed. Where that directory
  cannot be read or has been removed, the profile runtime also prints
  `LLVM Profile Error: Failed to write file` on stderr, and four tests in `tests/cli.rs` that
  expect exactly one stderr line failed: the coverage run stopped after 78 tests with those
  4 failed.
- `clap_complete_command` 0.6.1 is the latest release on crates.io; it brings
  `clap_complete` 4.6.11 and, through its default `nushell` feature, `clap_complete_nushell`
  4.6.2. The value names of its `Shell` are `bash`, `elvish`, `fish`, `nushell`, `powershell`
  and `zsh`.
- The output contract of the plan fixes the `import` data as `{created: [ids]}`, but the
  command serialized `{ids: [...]}`. Field names are a public contract, so the field is
  renamed to `created` and the guide and README document `{created}`.

### Proposal

- Add `src/guide.rs` with the guide as `include_str!("../docs/guide.md")` and the completion
  script generated from `Cli::command()`, both returned as `{text}`; add the `guide` and
  `completions` subcommands; render the text through `output::document_text`.
- Run each guide line with `sh -c` through the shared helper, with `PATH` holding the
  directory of the binary under test first; the test first checks that `command -v tk`
  names that binary. Guide lines may not mention `TASKIST_DB`, `HOME`, `XDG_DATA_HOME` or
  `cd`, so they reach only the temporary database.
- Pass `LLVM_PROFILE_FILE` from the test process to every child in the shared helper; the
  confinement tests of `tests/sandbox.rs` leave out exactly the passed-on value and still
  require every other variable to be the sandbox set inside the sandbox root.
- The concurrency test runs two threads, each starting 50 `tk add` processes one after the
  other behind a barrier, and checks that the two loops overlapped in time.

### Results

- Failing tests first, run before the implementation:
  - `tests/guide.rs`: six of seven tests failed with `error: unrecognized subcommand 'guide'`
    or `'completions'` (exit 2); the reader test of the `sh` blocks passed.
  - `tests/docs.rs`: `every_mvp_criterion_maps_to_tests_that_exist` failed with
    `an ## MVP acceptance map section`, and
    `the_readme_says_backups_are_copies_of_the_database_file` failed on the old README; the
    reader control passed.
  - `tests/concurrency.rs` passed on its first run: the store already sets WAL and a busy
    timeout. Its control `the_lock_complaint_reader_recognizes_the_sqlite_messages` shows the
    stderr reader finds the SQLite `database is locked` message.
- After the implementation: `cargo fmt --all --check`, `cargo clippy --all-targets --locked`
  and `cargo nextest run --locked --no-tests=pass` pass; nextest reported 215 passed,
  0 skipped. `cargo test --doc --locked` reported 8 doctests passed. `typos`, `cargo shear` and
  `cargo deny check` pass.
- `cargo llvm-cov nextest --locked --summary-only` ran 215 tests, all passed, and reported
  97.91% line coverage (3725 lines, 78 missed), 94.16% region coverage and 95.88% function
  coverage.
- `import` data renamed to `{created}`: with the four `tests/exchange.rs` expectations changed
  to `{"created": [...]}` first, `import_creates_one_task_per_line_in_file_order`,
  `import_reads_stdin_and_uses_the_resolved_scope` and `an_empty_import_creates_nothing`
  failed with `left: Object {"ids": ...}`, `right: Object {"created": ...}`; they pass after
  the rename.
- Under the load of the full test stage
  (`cargo nextest run --all-features --locked --no-tests=fail --run-ignored all --no-fail-fast`),
  run 10 consecutive times, every run passed all 215 tests with 0 skipped,
  `tests/concurrency.rs` included. `cargo llvm-cov nextest --locked --summary-only` on the same
  commit reported 215 passed and 97.91% line coverage (3725 lines, 78 missed).
