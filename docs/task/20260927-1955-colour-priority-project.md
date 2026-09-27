# 20260927-1955-colour-priority-project Colour priorities and project names in human output

- **status**: completed
- **priority**: P2
- **owner**: agent-a/session-20260927-1955
- **createdAt**: 2026-09-27 19:55

## Description

Human output colours only the status word and bolds headings. Add colour to the
priority (`P0`..`P3`) and to project names, under the existing colour rules
(`Env::colour`); JSON output stays uncoloured.

Acceptance criteria:

- Priorities are styled in task lines (`tk ls`, `tk find`, `tk next`) and in the
  `tk show` priority field.
- Project names are styled wherever list and show views print them: `tk ls`,
  `tk find`, `tk next`, `tk show`, `tk project ls`, `tk project show`, `tk feature ls`.
- With colour disabled the output is byte-for-byte unchanged.
- Unit tests cover the new styles; the full quality gate passes.

## ActiveForm

Colouring priorities and project names

## Dependencies

- **blocked by**: (none)
- **blocks**: (none)

## Notes

(none)

- complete: Unit tests added first (RED on missing API), then GREEN; full quality gate passed.
