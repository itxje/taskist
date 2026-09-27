# 20260927-1230-repository-foundation Add repository foundation and the tk entry point

- **status**: completed
- **priority**: P1
- **owner**: (unassigned)
- **createdAt**: 2026-09-27 12:30

## Description

Create the single package `taskist` (library `src/lib.rs`, binary `tk` from `src/main.rs`)
with the toolchain, lint, profile, dependency, deny, fmt, clippy and nextest configuration,
the repository hygiene files, the bundled-SQLite decision record, and the `error`, `env`,
`output` and `cli` modules behind a thin `main`. Part of plan
`docs/plan/20260927-1154-taskist-mvp.md` (sections 5 and 6).

Acceptance:

- `tk --version` prints the package version and exits 0; `tk --help` names `tk`.
- An unknown subcommand exits 2 with `error:` on stderr; with `--json` or
  `TASKIST_FORMAT=json` stderr is one JSON line with `ok` false and code `usage`, stdout empty;
  `TASKIST_FORMAT=xml` exits 2.
- Every error variant maps to its code and exit code.
- Database location resolution: `TASKIST_DB`, absolute `XDG_DATA_HOME`, relative
  `XDG_DATA_HOME` ignored, `HOME` only, nothing set.
- Only `main` reads the process environment and current directory.
- Both crate roots forbid unsafe code; the lint table matches the policy.
- The integration-test helper cannot reach a default database location.
- `LICENSE` is MIT with `Copyright (c) 2026 taskist contributors`; the decision record has
  context, decision, consequences and sunset condition.

## ActiveForm

Adding the repository foundation and the tk entry point

## Dependencies

- **blocked by**: (none)
- **blocks**: (none)

## Notes

### Investigation

- The repository held only PMA docs and a `.gitignore` with `/target`.
- The build host has Rust 1.96.0, cargo-nextest, cargo-deny, cargo-shear and typos.
- crates.io on 2026-09-27 lists the same latest stable versions the design names: clap 4.6.7,
  clap_complete_command 0.6.1, rusqlite 0.40.2, serde 1.0.229, serde_json 1.0.151,
  thiserror 2.0.21, anyhow 1.0.104, anstream 1.0.0, anstyle 1.0.14, assert_cmd 2.2.2,
  predicates 3.1.4, tempfile 3.27.0.
- Each dependency enters `Cargo.toml` with the change that first uses it. This change declares
  `anyhow`, `clap` (features `derive`, `env`), `serde` (feature `derive`), `serde_json` and
  `thiserror`, and the dev-dependencies `assert_cmd`, `predicates` and `tempfile`. `rusqlite`
  (with its `# JUSTIFICATION:` comment) comes with the SQLite store, `anstream` and `anstyle`
  with colour output, and `clap_complete_command` with shell completions. The normal
  dependency tree has 31 packages.

### Design

- `Env` holds the variables (`OsString` map), the current directory and whether stdout is a
  terminal. `Env::database_path` applies the location order; empty values count as unset, and
  `HOME` must be absolute like `XDG_DATA_HOME`, so the location never depends on the current
  directory. With nothing usable the error is `usage` and names `TASKIST_DB`.
- `Error` is one thiserror enum; `code()` and `exit_code()` are exhaustive matches. A clap
  parsing error converts to `Usage` with clap's text minus its `error: ` prefix, so text mode
  prints clap's usage hint unchanged and JSON mode carries it in `message`.
- `output::Format::detect` implements `--json` > `TASKIST_FORMAT` (`json`/`text`) > text.
  `render_success` / `render_failure` build the envelopes; `emit_*` are the only writers to
  stdout and stderr and use `write_all`, so a closed pipe becomes an error instead of a panic.
- `run` parses with clap: help and version go to stdout with exit 0; any other parsing error
  goes through `fail`, which picks JSON when `--json` is among the raw arguments or
  `TASKIST_FORMAT=json`, and reports an invalid `TASKIST_FORMAT` itself as a text usage error.
- `main` collects arguments and variables, reads the current directory with anyhow context,
  and on failure reports an `internal` error through `fail`.
- `tk` without a subcommand is a usage error (`arg_required_else_help = false`) rather than
  help printed as an error.
- Integration tests are marked `#![cfg(test)]` so the `clippy.toml` test allowances apply to
  their helper functions; `tests/common` is `pub mod` so `unreachable_pub` and
  `clippy::redundant_pub_crate` do not conflict.
- The helper runs `tk` with `env_clear`, `TASKIST_DB`, `HOME`, `XDG_DATA_HOME` and the working
  directory inside a fresh temporary directory; `assert_cmd` hands the child an empty stdin
  pipe that is closed before waiting unless a test supplies input.
- No `_typos.toml`: `typos` reports nothing on the repository.

### Results

- Failing first: before the implementation, 25 of 39 tests failed, for example
  `every_variant_maps_to_its_documented_code_and_exit_code` with
  `left: [("internal", 1), ("internal", 1), ...]`, and every binary test because `run` did
  nothing. The test for the `rusqlite` justification comment was later removed together with
  the dependency; it returns with the SQLite store.
- `cargo fmt --all --check && cargo clippy --all-targets --locked && cargo nextest run --locked
  --no-tests=pass`: passed, no warnings, 38 tests run, 38 passed, 0 skipped.
- `cargo shear`: no issues found.
- `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok.
- `typos`: no findings.
- The sandbox test currently finds no database file, because no command opens the database
  yet; it asserts that the resolved location lies inside the temporary directory and that any
  database file created is that one.
