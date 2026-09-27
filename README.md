# taskist

`tk` tracks unfinished and follow-up work for many projects in one central SQLite database,
grouped as project -> feature -> task. It is meant to be equally usable by a person at a
terminal and by an automated agent through a stable, non-interactive JSON contract.

The full usage guide ships inside the binary: `tk guide` prints it, and it is kept in
`docs/guide.md`. Point a project's `AGENTS.md` or an agent skill at `tk guide`.

## Install from source

Requires the Rust toolchain pinned in `rust-toolchain.toml` and a C compiler, because SQLite
is compiled from source and linked statically (see
`docs/decisions/2026-09-27-bundled-sqlite.md`).

```bash
cargo install --path . --locked
tk --version
```

Or build without installing:

```bash
cargo build --release --locked
./target/release/tk --help
```

Shell completions: `tk completions bash > ~/.local/share/bash-completion/completions/tk`
(also `elvish`, `fish`, `nushell`, `powershell`, `zsh`).

## Quick start

```bash
cd ~/src/web
tk project add web --path .          # commands run here now target `web`
tk add "Fix login redirect" -f auth --pri 1 --tag bug
tk add "Write onboarding docs" -f docs
tk ls                                # open tasks, grouped by feature
tk start 1
tk done 1 "Redirect now keeps the target page"
tk brief                             # markdown digest of open work
tk ls --all-projects --json          # everything open, as one JSON envelope
```

## Environment variables

| Variable | Effect |
|---|---|
| `TASKIST_DB` | Database file, used as given. |
| `XDG_DATA_HOME` | Without `TASKIST_DB`: `$XDG_DATA_HOME/taskist/taskist.db` when absolute. |
| `HOME` | Otherwise: `$HOME/.local/share/taskist/taskist.db` when absolute. |
| `TASKIST_PROJECT` | Project of commands run without `-p/--project`. |
| `TASKIST_FORMAT` | `json` or `text`; `--json` wins; any other value is a `usage` error. |
| `TASKIST_ACTOR` | Actor recorded on tasks and notes without `--by`; then `USER`, then `unknown`. |
| `NO_COLOR`, `CLICOLOR`, `CLICOLOR_FORCE`, `TERM`, `CI` | Colour of human output, by the usual rules; JSON is never coloured. |

An empty `TASKIST_DB`, `XDG_DATA_HOME`, `HOME`, `TASKIST_PROJECT`, `TASKIST_ACTOR`,
`USER`, `NO_COLOR` or `CLICOLOR_FORCE` counts as unset. An empty `CLICOLOR`, `TERM` or `CI`
counts as set. An empty `TASKIST_FORMAT` is a `usage` error, like any value other than
`json` or `text`.

The project scope is, first match wins: `-p/--project`,
`TASKIST_PROJECT`, the project whose `--path` contains the current directory, none. Listing
commands without a scope, or with `--all-projects`, cover every non-archived project.

## Command reference

| Command | Purpose |
|---|---|
| `tk add TITLE [-p P] [-f F] [--pri N] [--tag T]... [--body TEXT\|-]` | Create a task. |
| `tk ls [-p P] [--all-projects] [-f F] [--status S,S] [--tag T] [--all] [--limit N]` | List tasks. |
| `tk show ID` | Show a task and its notes. |
| `tk edit ID [--title T] [--body TEXT\|-] [--pri N] [-f F \| --no-feature] [--tag [+\|-]T]... [-p P]` | Change a task. |
| `tk start ID...`, `tk block ID REASON`, `tk done ID... [NOTE]`, `tk drop ID REASON`, `tk reopen ID...` | Change status. |
| `tk note ID TEXT` | Append a note. |
| `tk next [-p P] [--all-projects] [-f F]` | The open task to work on next. |
| `tk find QUERY [-p P] [--all-projects] [--all]` | Search titles, bodies and notes, ignoring case. |
| `tk brief [-p P] [--all-projects]` | Markdown digest of open work. |
| `tk export [--format json\|md] [-p P]` | Print every project, feature, task and note. |
| `tk import FILE\|-` | Create tasks from JSON lines, all or none. |
| `tk project add\|ls\|show\|edit\|archive\|rm` | Manage projects. |
| `tk feature ls\|mv` | List, rename or merge features. |
| `tk guide` | Print the usage guide. |
| `tk completions SHELL` | Print a shell completion script. |

Global options: `--json`, `--by ACTOR`, `--help`, `--version`. `tk guide` documents every
option, the status transitions and each command's JSON data.

## JSON contract

- `--json` or `TASKIST_FORMAT=json` selects JSON.
- Success: exactly one line `{"ok":true,"data":...}` on stdout, nothing on stderr.
- Failure: exactly one line `{"ok":false,"error":{"code":"...","message":"..."}}` on stderr,
  nothing on stdout. Human failures print `error: <message>` on stderr.
- Argument errors exit 2 and use the JSON envelope when `--json` is among the arguments or
  `TASKIST_FORMAT=json` is set. `--help` and `--version` print text and exit 0.
- A task is `{id, project, feature, title, body, status, priority, tags, created_at,
  updated_at, closed_at, created_by}`; `tk import` returns `{created: [ids]}`. Field names
  are stable; changes only add fields.
- Status changes are idempotent: `tk done 3` on a done task succeeds without a change.

## Exit codes

| Exit | Error codes | Meaning |
|---|---|---|
| 0 | | success |
| 1 | `internal`, `unsupported_schema` | unexpected failure; database written by a newer version |
| 2 | `usage` | bad arguments or values, missing scope, malformed import line |
| 3 | `not_found` | unknown task id, project or feature |
| 4 | `conflict`, `invalid_transition` | duplicate or archived project, non-empty removal; forbidden status change |

## Backups

A backup is a copy of the database file: copy it while no `tk` command runs, together with
any `-wal` file next to it. There is no restore command: `tk export` writes a report in JSON or
markdown that cannot be imported back (`tk import` creates new tasks from JSON lines, and does
not read an export).

## Development

```bash
cargo fmt --all --check
cargo clippy --all-targets --locked
cargo nextest run --locked
cargo test --doc --locked
cargo llvm-cov nextest --locked --summary-only
cargo deny check
```

Tests never touch a real database: integration tests run `tk` with an empty environment inside
a temporary directory.

## License

MIT, see `LICENSE`.
