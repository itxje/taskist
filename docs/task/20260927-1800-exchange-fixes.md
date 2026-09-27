# 20260927-1800-exchange-fixes Fix the brief reason layout, import error line numbers and import scope resolution

- **status**: completed
- **priority**: P1
- **owner**: (unassigned)
- **createdAt**: 2026-09-27 18:00

## Description

Close three defects an independent check of `brief`, `export` and `import` found against
sections 3.7, 3.8 and 3.9 of plan `docs/plan/20260927-1154-taskist-mvp.md`.

Acceptance:

- A blocked task whose reason is `first\n# api (9 open)\n- #1 forged` appears in `brief`
  (text and `--json` `data.text`) with the second and third lines indented under its item, so
  the stored text creates no heading or top-level list item of the digest.
- A malformed import line 3 exits 2 with a message that begins `line 3:` and names no other
  line number; the same holds for an unknown field and a missing title on line 2.
- `tk import -p nosuch FILE`, with every line naming an existing project, exits 3 with
  `not_found` and creates no task; the same with `TASKIST_PROJECT=nosuch`.
- Lines without `project` and no resolvable scope still exit 2 naming `-p/--project`.

## ActiveForm

Fixing the brief reason layout, import error line numbers and import scope resolution

## Dependencies

- **blocked by**: `20260927-1700-brief-export-import`
- **blocks**: (none)

## Notes

### Investigation

- `brief` wrote the latest blocked reason verbatim, so its continuation lines started at
  column 0 and a line such as `# api (9 open)` became a project heading of the digest, while
  `export --format md` already indents continuation lines with `continued`.
- A line that failed to parse carried `serde_json`'s own message, which ends in
  `at line 1 column N`: each import line is parsed on its own, so the parser always counts
  line 1, contradicting the `line N:` prefix naming the line of the file.
- `import` resolved the scope only for the first line without a `project`, so an unknown
  project named by `-p` or `TASKIST_PROJECT` went unnoticed when every line named its own;
  every other command refuses it with `not_found`.

### Proposal

- Render the reason through `continued(reason, "  ")`, the helper the md export uses.
- Map a parse error to `column N: <message>`: the message with the parser's exact
  ` at line L column C` suffix removed, so the only line number is the file's.
- When `-p` or `TASKIST_PROJECT` names the project, resolve and require it inside the write
  transaction before any line is parsed. Directory scope stays lazy: the current directory is
  read only for the first line without a project, and a missing scope is still the `usage`
  error naming `-p/--project`.

### Results

- Failing tests first, each run before the fix:
  - `a_multi_line_blocked_reason_stays_inside_its_item`: the continuation lines `# api (9 open)`
    and `- #1 forged` were printed at column 0.
  - `an_import_error_names_only_the_line_of_the_file`: line numbers `["3", "1"]`, expected
    `["3"]`.
  - `import_refuses_an_unknown_project_flag_or_variable`: exit 0 with `{"ids":[1]}`, expected
    exit 3.
- After the fix these pass together with the unit test
  `parse_errors_carry_a_column_and_no_line` and the unchanged
  `import_without_a_resolved_project_names_the_flag`.
- `cargo fmt --all --check`, `cargo clippy --all-targets --locked` and
  `cargo nextest run --locked --no-tests=pass` pass; nextest reported 199 passed, 0 skipped.
