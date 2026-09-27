# 20260927-2005-table-task-list Render task lists as an aligned table

- **status**: completed
- **createdAt**: 2026-09-27 20:05
- **approvedAt**: 2026-09-27 20:20
- **relatedTask**: 20260927-2005-table-task-list

## Context

- `output::task_list_text` renders `tk ls` and `tk find`: a `name  (N open)` heading
  per project, an indented heading per feature, and `#id  P<n>  status  title` lines.
- `task_line` pads the status column but not the id, so `#3` and `#10` in one list
  shift the priority, status and title columns by one character.
- Nested headings put the project and feature on separate lines from the task, so a
  line of the list alone does not say where the task belongs.
- Project, feature and tag names are `[a-z0-9][a-z0-9-]*` (`model::validate_name`), so
  every column except the title is ASCII and its width is its byte length.
- Colour is added by `Paint` around the text; padding must be computed on the plain
  text and added outside the style (as `status_column` already does).
- `task::list` filters in Rust over `TaskView`s; `created_at` is stored as a UTC
  `YYYY-MM-DDTHH:MM:SS.sssZ` string written by SQLite (`strftime(..., 'now')`), so
  such strings compare correctly as text. There is no date/time crate; SQLite's date
  functions (including the `utc` modifier, which reads the system time zone) are available
  inside the read transaction.
- Tests that pin the current layout: `tests/task.rs` (4 assertions), `tests/scope.rs`
  (1 assertion), unit tests in `src/output.rs`. The README describes `tk ls` as
  "grouped by feature". JSON output is not affected.

## Proposal

Render `tk ls` and `tk find` as one borderless table with a header row:

```text
ID   PRI  STATUS   PROJECT    FEATURE              AGE  TITLE
#1   P0   todo     ns-remote  ci                   5d   Gate the Unix-only RawInjected test helper
#5   P2   todo     ns-remote  keynet               3d   Find the cause of the two-minute window
#2   P1   todo     nsgw       ci                   2h   Check the CI result for nsgw main
#3   P2   todo     nsgw       keynet-relay-region  12d  Decide whether a dead member is detected faster
#10  P2   todo     nsgw       keynet-relay-region  40m  Serve region quick pairing from one owner
#11  P2   blocked  nsgw       api                  1d   Paginate /orders  (waiting on schema)
```

- Columns: `ID`, `PRI`, `STATUS`, `PROJECT`, `FEATURE`, `AGE`, `TITLE`. Each column except the
  title is padded to its widest cell (header included) and separated by two spaces;
  the title is last and not padded.
- `PROJECT` is omitted when the list is scoped to one project (flag, env or directory),
  since every row would repeat it.
- A task without a feature shows `-`.
- Row order stays as today: by project, then feature group, then the existing order
  inside a group.
- A blocked task keeps its latest reason after the title in parentheses.
- Colour: header bold; priority, status and project styled as today; padding outside
  styles. Without colour the output is plain aligned text.
- The per-project `(N open)` heading is dropped; `tk project ls` keeps the counts.
- An empty result keeps printing `no tasks`.
- `tk next` keeps its single line and `tk show` is unchanged.

Creation time:

- `AGE` is the time since the task was created: `Nm` under an hour, `Nh` under a day,
  `<N>d` otherwise (whole units, rounded down). It is time-zone free and reads the same way
  as the `--since` values. `tk show` keeps the full `created:` timestamp.
- The current time is read once per command from the store (`Tx::now`) and carried in
  `TaskList` as a `#[serde(skip)]` field, so JSON output is unchanged and tests can
  render with a fixed time.

Time filter:

- `tk ls --since <WHEN>` keeps tasks created at or after `WHEN`. It combines with every
  other filter (`--status`, `--all`, `-f`, `--tag`, `--limit`).
- `WHEN` is either a duration back from now, `<N>m`, `<N>h`, `<N>d` or `<N>w`
  (`--since 3d` = the last 72 hours), or a local calendar date `YYYY-MM-DD`, meaning
  local midnight at its start (`--since 2026-09-20`).
- The cutoff is computed by SQLite in the read transaction (`strftime` with a `-N days`
  style modifier, or with the `utc` modifier for a local date) and compared with
  `created_at` as text. No new dependency.
- Anything else, or `N` of zero or more digits than fit, is a `usage` error (exit 2).
- `docs/guide.md` and the README document the flag and its values.

Implementation: a small table helper in `src/output.rs` that takes rows of
`(plain, styled)` cells, computes widths from the plain text, and pads outside the
style. No new dependency.

Acceptance criteria:

1. Ids of different widths align every following column (unit and integration tests
   with `#9` and `#10` in one list).
2. `PROJECT` appears only for lists not scoped to one project.
3. Coloured output pads outside the styles; widths match the uncoloured output.
4. `AGE` renders `m`/`h`/`d` boundaries correctly (unit tests with a fixed time).
5. `--since` with each unit and with a date keeps exactly the tasks at or after the
   cutoff (integration tests seed `created_at` values directly in a temporary database);
   invalid values fail with `usage`.
6. Updated integration tests, README and guide wording, and the full quality gate pass.

## Risks

- Human output layout changes; scripts that parsed the old text break. JSON is the
  documented machine contract, so this is acceptable.
- Long titles still wrap at the terminal edge; they are the last column, so the other
  columns stay aligned.

## Scope

`src/output.rs`, `src/cli.rs`, `src/lib.rs`, `src/command/task.rs`, `src/store/query.rs`,
`tests/task.rs`, `tests/scope.rs`, `README.md`, `docs/guide.md`.

## Alternatives

| Option | Pros | Cons |
|---|---|---|
| Aligned borderless table (recommended) | Every row self-contained and greppable; aligned; no dependency | Project and feature repeat on each row |
| Keep the tree, only pad the id column | Smallest change | Rows still lack project and feature; deep nesting stays |
| Boxed table (`comfy-table`), wrapped to terminal width | Looks like a spreadsheet, wraps long titles in the cell | New dependency and terminal-width detection; wider; harder to grep |
| `CREATED` as a local date (`2026-09-25`) instead of `AGE` | Exact day | Needs time-zone conversion (a date crate or per-row SQL); wider; does not match `--since` durations |
| `--since` on `find` and `brief` too | Consistent | Not asked for; can follow later |
| Table with repeated project/feature cells blanked | Less visual noise | A row alone no longer says where it belongs; grep loses context |

## Annotations

(none)
