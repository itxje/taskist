# Taskist - Task List

> Updated: 2026-09-27

## Usage

Each task is a single line linking to its detail file. All detailed information lives in `docs/task/<timestamp>-<feature-slug>.md`.

### Format

- [ ] [**20260907-1428-add-endpoint Add endpoint**](20260907-1428-add-endpoint.md) `P1`

### Status Markers

| Marker | Meaning |
|--------|---------|
| `[ ]`  | Pending |
| `[-]`  | In progress |
| `[x]`  | Completed |
| `[~]`  | Closed / Won't do |
| `[d]`  | Deleted detail file; index entry retained |

### Priority: P0 (blocking) > P1 (high) > P2 (medium) > P3 (low)

### Rules

- Only update the checkbox marker; never delete the line or change its other content. If the detail file is deleted, mark the entry `[d]`.
- Record change history and deletion reasons in `docs/changelog.md`; update affected dependency and plan references.
- New tasks append to the end.
- See each `<timestamp>-<feature-slug>.md` for full details, except `[d]` entries whose files have been deleted; consult `docs/changelog.md` for their history.

---

## Tasks

- [-] [**20260927-1154-taskist-mvp Design and build the taskist CLI MVP**](20260927-1154-taskist-mvp.md) `P1`
- [x] [**20260927-1230-repository-foundation Add repository foundation and the tk entry point**](20260927-1230-repository-foundation.md) `P1`
- [x] [**20260927-1300-sqlite-store Add the SQLite store and the domain model**](20260927-1300-sqlite-store.md) `P1`
- [x] [**20260927-1344-scope-projects-features Add scope resolution and the project and feature commands**](20260927-1344-scope-projects-features.md) `P1`
- [x] [**20260927-1409-task-commands Add the task create, list, show, edit, next and find commands**](20260927-1409-task-commands.md) `P1`
- [x] [**20260927-1530-task-command-fixes Fix find case folding, dash values, filter names, colour rules and the ls age order**](20260927-1530-task-command-fixes.md) `P1`
- [x] [**20260927-1620-status-transitions Add the status transition and note commands and validate lookup names**](20260927-1620-status-transitions.md) `P1`
- [x] [**20260927-1700-brief-export-import Add brief, export and import**](20260927-1700-brief-export-import.md) `P1`
- [x] [**20260927-1800-exchange-fixes Fix the brief reason layout, import error line numbers and import scope resolution**](20260927-1800-exchange-fixes.md) `P1`
- [x] [**20260927-1900-guide-docs-coverage Add the guide, completions, documentation, concurrency test and coverage**](20260927-1900-guide-docs-coverage.md) `P1`
