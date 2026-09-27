# Changelog

## 2026-09-27 11:54 [progress]

Initialized PMA docs. Drafted the MVP plan `20260927-1154-taskist-mvp` (project -> feature -> task model, SQLite store, agent-friendly CLI contract); awaiting approval.

## 2026-09-27 12:05 [decision]

Plan `20260927-1154-taskist-mvp`: switched implementation language from Go to Rust at the user's request. Storage uses `rusqlite` (bundled, synchronous); the `libsqlite3-sys` C dependency will be recorded as a baseline exception in `docs/decisions/`.

## 2026-09-27 12:10 [decision]

Plan `20260927-1154-taskist-mvp` approved: binary `tk`, five-status set, explicit project creation.
