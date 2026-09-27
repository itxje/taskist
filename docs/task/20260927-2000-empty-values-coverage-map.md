# 20260927-2000-empty-values-coverage-map Document which empty environment values count as unset and name the coverage measurement in the acceptance map

- **status**: completed
- **priority**: P1
- **owner**: (unassigned)
- **createdAt**: 2026-09-27 20:00

## Description

The guide and the README said "Empty values count as unset" under the environment-variable
table, but the colour rule follows `anstream`: an empty `CLICOLOR`, `TERM` or `CI` counts as
set, so it allows colour on a terminal, and an empty `CLICOLOR` does so even with
`TERM=dumb`. The guide documents the public contract, so the sentence must match the code.
Criterion 6 of the acceptance map in `docs/architecture.md` (at least 80% line coverage;
lint clean) named only the lint policy tests, none of which shows the coverage figure.

Acceptance:

- Unit tests on hand-built `Env` values pin the empty-value behaviour of each colour
  variable: empty `NO_COLOR` and `CLICOLOR_FORCE` count as unset; on a terminal, empty
  `TERM` or `CI` allows colour, and empty `CLICOLOR` allows colour even with `TERM=dumb`.
- A docs test fails if the guide or the README contains the unqualified sentence, and checks
  that their empty-value statement names `CLICOLOR`, `TERM` and `CI` as the variables whose
  empty value counts as set.
- Criterion 6 of the acceptance map names the lint tests, the `cargo llvm-cov nextest`
  measurement over the whole package and the figure recorded in `docs/changelog.md`; every
  test the map names still exists.

## ActiveForm

Documenting empty environment values and the coverage entry of the acceptance map

## Dependencies

- **blocked by**: `20260927-1900-guide-docs-coverage`

## Investigation

- `src/env.rs` `Env::colour` reads `NO_COLOR` and `CLICOLOR_FORCE` through `non_empty`, but
  `CLICOLOR`, `TERM` and `CI` through `var`, so their empty values count as set. This mirrors
  `anstream`, which the plan names for the colour rule, so the code stays and the documents
  change.
- `TASKIST_DB`, `XDG_DATA_HOME`, `HOME`, `TASKIST_ACTOR` and `USER` are read through
  `non_empty` in `src/env.rs`; `TASKIST_PROJECT` is filtered for emptiness in `src/scope.rs`
  and `src/command/exchange.rs`.
- `TASKIST_FORMAT` is matched against `json` and `text` in `src/output.rs`; an empty value is
  neither, so `TASKIST_FORMAT= tk ls` exits 2 with
  `error: TASKIST_FORMAT must be json or text, got ""`. It does not count as unset.
- `tests/docs.rs` checked only that every test named in the acceptance map exists.

## Proposal

- Replace the blanket sentence in `docs/guide.md` and `README.md` with three statements:
  the variables whose empty value counts as unset, the colour variables whose empty value
  counts as set, and the empty `TASKIST_FORMAT` as a `usage` error.
- Extend the `Env::colour` documentation with the empty values that count as set.
- Add `src/env.rs::empty_colour_variables_follow_anstream`, with a calibration case first.
- In `docs/architecture.md`, split criterion 6 into a lint part (the policy tests) and a
  coverage part (the `cargo llvm-cov nextest` measurement over the whole package and the
  figure in `docs/changelog.md`).
- In `tests/docs.rs`, check the empty-value sentences of both documents against the exact
  variable lists, and check that criterion 6 names the measurement and the changelog and that
  the changelog records a figure of at least 80%.

## Results

- Failing tests first: before the document changes,
  `tests/docs.rs::the_guide_and_readme_name_which_empty_values_count_as_unset` failed with
  "docs/guide.md claims that every empty value counts as unset", and
  `tests/docs.rs::criterion_6_names_the_lint_tests_and_the_coverage_measurement` failed at
  its first check on the row text. Both pass after the change.
- `src/env.rs::empty_colour_variables_follow_anstream` passes against the unchanged colour
  rule; it pins existing behaviour that the documents now describe.
- `cargo fmt --all --check`, `cargo clippy --all-targets --locked` and
  `cargo nextest run --locked --no-tests=pass` pass: 219 tests run, 219 passed, 0 skipped.
