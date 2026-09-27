# Bundled SQLite through rusqlite

- **date**: 2026-09-27
- **status**: accepted

## Context

taskist keeps all projects, features, tasks and notes in one SQLite database that several
processes may write at the same time. The Rust baseline the project follows prefers pure-Rust
dependencies and asks for a recorded justification for every C dependency. No mature pure-Rust
engine reads and writes the SQLite file format with WAL mode, `busy_timeout` and the
transaction guarantees the concurrency design relies on. Linking the system SQLite library
would make the binary depend on whatever version the host provides.

## Decision

Use `rusqlite` with the `bundled` feature. `libsqlite3-sys` compiles the SQLite amalgamation
from C source during the build and links it statically, so every build embeds the same SQLite
version. The `rusqlite` entry in `Cargo.toml` carries a `# JUSTIFICATION:` comment that points
here.

## Consequences

- Building needs a C compiler on the build host; running the binary needs no SQLite library.
- The SQLite version changes only when the `rusqlite` / `libsqlite3-sys` versions change, which
  makes behaviour reproducible across hosts.
- The C code sits outside `#![forbid(unsafe_code)]`; its memory safety rests on SQLite's own
  testing and on `rusqlite`'s safe wrapper.
- SQLite security fixes reach users only through a dependency update and a rebuild.
- The SQLite public-domain dedication and the MIT licence of the Rust crates fit the licence
  policy in `deny.toml`.

## Sunset condition

Review this decision when a pure-Rust engine that is compatible with the SQLite file format,
WAL mode and multi-process locking reaches a stable release.
