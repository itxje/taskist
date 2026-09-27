# taskist

`tk` tracks unfinished and follow-up work for many projects in one central SQLite database,
grouped as project -> feature -> task. It is meant to be equally usable by a person at a
terminal and by an automated agent through a stable, non-interactive JSON contract.

The command surface is being built; this release provides the entry point, the output
contract and the database location rules.

## Build

Requires the Rust toolchain pinned in `rust-toolchain.toml`. Once the SQLite store lands,
SQLite is compiled from source and the build also needs a C compiler; see
`docs/decisions/2026-09-27-bundled-sqlite.md`.

```bash
cargo build --release
./target/release/tk --help
```

## Database location

The first match wins:

1. `TASKIST_DB`, used as given.
2. `$XDG_DATA_HOME/taskist/taskist.db` when `XDG_DATA_HOME` is an absolute path.
3. `$HOME/.local/share/taskist/taskist.db` when `HOME` is an absolute path.

Empty values count as unset. With none of them usable, commands fail with a `usage` error.
Back up the database by copying the file.

## Output contract

- `--json` or `TASKIST_FORMAT=json` selects JSON; `TASKIST_FORMAT=text` or no setting selects
  text. Any other `TASKIST_FORMAT` value is a `usage` error.
- JSON success: one line `{"ok":true,"data":...}` on stdout, nothing on stderr.
- JSON failure: one line `{"ok":false,"error":{"code":"...","message":"..."}}` on stderr,
  nothing on stdout.
- Text failure: `error: <message>` on stderr.
- `--help` and `--version` print text to stdout and exit 0.

| Code | Exit | Meaning |
|---|---|---|
| `internal` | 1 | unexpected failure: I/O, SQLite, corrupt data |
| `unsupported_schema` | 1 | database written by a newer version |
| `usage` | 2 | bad arguments, bad values, missing scope, malformed import line |
| `not_found` | 3 | unknown task id, project or feature |
| `conflict` | 4 | duplicate name or path, archived project, non-empty project removal |
| `invalid_transition` | 4 | status change the transition table forbids |

## Development

```bash
cargo fmt --all --check
cargo clippy --all-targets --locked
cargo nextest run --locked
cargo deny check
typos
```

Tests never touch a real database: integration tests run `tk` with an empty environment inside
a temporary directory.

## License

MIT, see `LICENSE`.
