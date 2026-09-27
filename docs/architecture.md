# Taskist Architecture

> Status: approved, implementing per `docs/plan/20260927-1154-taskist-mvp.md`.

Taskist (`tk`) is a single-binary Rust CLI that stores tasks for many projects in one
central SQLite database (`rusqlite`, bundled). Tasks are grouped as
project -> feature -> task.

```text
src/main.rs     entry point, error -> exit code mapping
src/cli.rs      clap command definitions
src/output.rs   human and JSON rendering
src/model.rs    domain types and status transitions
src/scope.rs    project resolution (flag > env > cwd mapping)
src/store/      SQLite connection, embedded migrations, queries
```

See the plan for the data model, command surface, and agent contract.
