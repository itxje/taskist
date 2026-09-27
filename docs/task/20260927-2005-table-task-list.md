# 20260927-2005-table-task-list Render task lists as an aligned table

- **status**: completed
- **priority**: P2
- **owner**: agent-a/session-20260927-2005
- **createdAt**: 2026-09-27 20:05

## Description

The human form of `tk ls` and `tk find` indents tasks under project and feature
headings and does not pad the id, so ids of different widths (`#3`, `#10`) shift
every following column. Render the list as an aligned, borderless table instead.
The table also shows each task's age, and `tk ls --since` keeps only tasks created
within a recent period or since a date.

Acceptance criteria are in `docs/plan/20260927-2005-table-task-list.md`.

## ActiveForm

Rendering task lists as an aligned table

## Dependencies

- **blocked by**: (none)
- **blocks**: (none)

## Notes

(none)

- complete: Tests written first (RED), full quality gate passed.
