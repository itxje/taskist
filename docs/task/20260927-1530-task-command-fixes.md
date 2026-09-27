# 20260927-1530-task-command-fixes Fix find case folding, dash values, filter names, colour rules and the ls age order

- **status**: completed
- **priority**: P1
- **owner**: (unassigned)
- **createdAt**: 2026-09-27 15:30

## Description

Close five defects an independent check of the task commands found against sections 3.6,
3.8 and 3.9 of plan `docs/plan/20260927-1154-taskist-mvp.md`.

Acceptance:

- `tk find Σ` and `tk find σ` both find a task titled `ΟΔΟΣ`; a mixed-case ASCII query still
  matches case-insensitively.
- `tk edit <id> --pri -1`, `tk add x --pri -1` and `tk ls --limit -1` exit 2 with a `usage`
  error naming the option and its value, not an unknown-option error.
- `tk ls --tag BAD_TAG`, `tk ls -f BAD_NAME` and `tk next -f BAD_NAME` exit 2 with `usage`.
- Colour: none on a non-terminal; colour on a terminal; none on a terminal with `NO_COLOR`,
  `CLICOLOR=0` or `TERM=dumb`; colour with `CLICOLOR_FORCE=1` on a non-terminal; none with
  `CLICOLOR_FORCE=1` and `NO_COLOR=1`.
- `ls` and `find` order by age between priority and id, shown by a test that fails when
  `created_at` is dropped from the ordering key.

## ActiveForm

Fixing find case folding, dash values, filter names, colour rules and the ls age order

## Dependencies

- **blocked by**: `20260927-1409-task-commands`
- **blocks**: (none)

## Notes

### Investigation

- `find` lowercased the query and each text with `str::to_lowercase`, which maps a capital
  sigma to `ς` at the end of a word and to `σ` elsewhere, so `Σ` alone never matched `ΟΔΟΣ`.
- `--pri` (on `add` and `edit`) and `--limit` (on `ls`) were the only value-taking options
  without `allow_hyphen_values`, so `-1` was parsed as an unknown option.
- `ls -f`, `ls --tag` and `next -f` looked the given name up without validating it: an invalid
  tag listed nothing (exit 0) and an invalid feature was `not_found` (exit 3).
- `Env::colour` checked only `NO_COLOR`, `CLICOLOR_FORCE` and the terminal, and the stream is
  written with `ColorChoice::Always`, so `anstream`'s own `TERM` and `CLICOLOR` rules never ran.
- The `ls` age key was correct but untested: every fixture created tasks in id order.

### Proposal

- Fold case without context: replace every character by the lowercase of its uppercase
  (`char::to_uppercase` then `char::to_lowercase`) and repeat until the text no longer
  changes, applied identically to the query and every text. This maps all three sigmas to
  `σ` and folds expansions such as `ß` to `ss`. One pass is not enough: the capital sharp s
  `ẞ` is its own uppercase and folds to `ß`, which folds on to `ss` in a second pass.
- Add `allow_hyphen_values` to `--pri` and `--limit`; the value parser then rejects `-1` as an
  invalid value of the option.
- Validate feature and tag filter names with `validate_name` before the store is opened.
- Make `Env::colour` apply the automatic rules of `anstream` 1.0 (`anstyle-query`) to the
  captured variables: `NO_COLOR` (non-empty) off; else `CLICOLOR_FORCE` (non-empty) on; else
  `CLICOLOR=0` off; else on only for a terminal with `TERM` set and not `dumb`, `CLICOLOR` set,
  or `CI` set. The writer keeps `ColorChoice::Always`, so the library still reads no process
  environment; `Paint` adds styles only when `Env::colour` allows them.
- Add integration tests for each finding and an `ls`/`find` fixture where the lower id is the
  younger task.

### Results

- Failing first: `find_folds_case_per_character_including_final_sigma` (`left: []`,
  `right: [1]` for `Σ`), `numeric_options_take_values_beginning_with_a_dash` (`unexpected
  argument '-1' found`), `invalid_feature_and_tag_filters_are_usage_errors` (exit 0 instead of
  2). `ls_and_find_order_by_age_between_priority_and_id` passed on the unchanged code and failed
  (`left: [3, 1, 2]`, `right: [3, 2, 1]`) with `created_at` replaced by a constant in the
  ordering key.
- Unit tests: `colour_follows_the_automatic_rules_of_anstream` covers every rule on hand-built
  `Env` values; `case_folding_does_not_depend_on_the_position_in_a_word` covers the sigmas and
  the sharp s; `every_character_folds_like_its_case_variants_and_folding_is_stable` checks,
  over every Unicode scalar value, that folding is stable and that a character folds like its
  lowercase and its uppercase. With a single pass it fails at U+1E9E (`ss` against `ß`), and so
  does `find_matches_the_capital_sharp_s_and_its_lowercase` (`tk find groß` finds nothing).
- On a pseudo-terminal, `tk ls` prints escape sequences with `TERM=xterm-256color` and none
  with `TERM=dumb` or `CLICOLOR=0`.
- `cargo fmt --all --check`, `cargo clippy --all-targets --locked` and
  `cargo nextest run --locked --no-tests=pass`: 155 passed, 0 skipped. `typos`,
  `cargo shear` and `cargo deny check` report no issues.
- `fold_case` is the only case-insensitive comparison of user text; the other one in the
  library compares the SQLite journal mode with `wal`.
