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
- `continued`, which keeps multi-line stored text inside its list item in the brief and in
  the md export (project description, task body, note text), broke only at line feeds. A
  bare carriage return is a line ending for a `CommonMark` reader, so a reason
  `first\r# api (9 open)` or a body `x\r### forged` still formed a top-level heading.

### Proposal

- Render the reason through `continued(reason, "  ")`, the helper the md export uses.
- Make `continued` break at every line terminator of the set the model uses to keep titles
  single-line (LF, VT, FF, CR, NEL, U+2028, U+2029, with CRLF as one break), writing each
  break as a line feed followed by the indent. The model keeps its set private, so the same
  set is defined next to `continued` with a comment naming the model's.
- Stored text also reaches human (non-markdown) output: the `ls` reason
  (`src/output.rs` around line 254), the `show` body and note text (around lines 311 and
  325) and the `project show` description (around line 411). These are plain text, not
  markdown, and are left as they are.
- Map a parse error to `column N: <message>`: the message with the parser's exact
  ` at line L column C` suffix removed, so the only line number is the file's.
- Keep the order of the import: read the file, parse and validate every line, and only then
  open the store, so a bad line is a `usage` error that neither creates the database file
  nor depends on the database being readable. Inside the write transaction, before any task
  is created, resolve and require the project named by `-p` or `TASKIST_PROJECT`. Directory
  scope stays lazy: the current directory is read only for the first line without a
  project, and a missing scope is still the `usage` error naming `-p/--project`.
- Whether the flag or the variable names the project is decided in `import` with the same
  condition `scope::target` uses, because the target keeps its choice private; an accessor
  on the target in `src/scope.rs` would remove this duplication.

### Results

- Failing tests first, each run before the fix:
  - `a_multi_line_blocked_reason_stays_inside_its_item`: the continuation lines `# api (9 open)`
    and `- #1 forged` were printed at column 0.
  - `an_import_error_names_only_the_line_of_the_file`: line numbers `["3", "1"]`, expected
    `["3"]`.
  - `import_refuses_an_unknown_project_flag_or_variable`: exit 0 with `{"ids":[1]}`, expected
    exit 3.
  - `a_malformed_import_is_refused_before_the_database_is_opened`: with the scope resolved
    before the lines were parsed, a fresh environment reported `not_found` (exit 3) for the
    named project and had created the database file.
  - `a_malformed_import_is_a_usage_error_on_a_database_that_cannot_be_opened`: on a database
    with `user_version` 99 the import reported `unsupported_schema` (exit 1), not `usage`.
  - `a_blocked_reason_breaks_at_every_line_terminator_inside_its_item`: the line-feed
    control passed; with CRLF the reason kept its carriage returns, and with a bare CR its
    lines formed top-level blocks.
  - `export_md_keeps_stored_text_inside_its_item_at_every_line_terminator`: the line-feed
    control passed; with CRLF the description continuation was not a plain indented line.
- After the fix these pass together with the unit test
  `parse_errors_carry_a_column_and_no_line` and the unchanged
  `import_without_a_resolved_project_names_the_flag`.
- `cargo fmt --all --check`, `cargo clippy --all-targets --locked` and
  `cargo nextest run --locked --no-tests=pass` pass; nextest reported 203 passed, 0 skipped.
- `typos`, `cargo shear` and `cargo deny check` pass.
