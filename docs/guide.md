# tk guide

`tk` tracks unfinished and follow-up work for many projects in one SQLite database, grouped
as project -> feature -> task. Every command is non-interactive, runs one transaction, and
prints either human text or one JSON envelope. This guide is printed by `tk guide`; the
examples below run in order against an empty database, from a directory that becomes the
`web` project.

## Environment variables

| Variable | Effect |
|---|---|
| `TASKIST_DB` | Database file, used as given; a relative path is relative to the current directory. |
| `XDG_DATA_HOME` | Without `TASKIST_DB`: the database is `$XDG_DATA_HOME/taskist/taskist.db` when this is absolute. |
| `HOME` | Otherwise: the database is `$HOME/.local/share/taskist/taskist.db` when this is absolute. |
| `TASKIST_PROJECT` | The project of commands run without `-p/--project`. |
| `TASKIST_FORMAT` | `json` or `text`; `--json` wins. Any other value is a `usage` error. |
| `TASKIST_ACTOR` | The actor recorded on created tasks and notes when `--by` is not given. |
| `USER` | The actor when neither `--by` nor `TASKIST_ACTOR` is set; the last fallback is `unknown`. |
| `NO_COLOR` | Set and non-empty: human output is never coloured. |
| `CLICOLOR_FORCE` | Set and non-empty: human output is always coloured (unless `NO_COLOR`). |
| `CLICOLOR` | `0` turns colour off; any other value allows it on a terminal. |
| `TERM` | Colour on a terminal needs `TERM` set to anything but `dumb`, or `CLICOLOR`, or `CI`. |
| `CI` | Set: colour is allowed on a terminal. |

Empty values count as unset. With no usable database location, commands that need the
database fail with `usage`. The database is created on first use. To back it up, copy the
file; `tk export` cannot be imported back.

## Scope resolution

Commands that act on a project find it in this order, first match wins:

1. `-p NAME` / `--project NAME`.
2. `TASKIST_PROJECT`.
3. The current directory: the project whose linked `--path` directory is the longest prefix of
   it, compared component by component on canonical paths. Archived projects never match.
4. None. Commands that create something (`add`, `import` lines without a project,
   `feature mv`) then fail with `usage`; listing commands (`ls`, `next`, `find`, `brief`,
   `feature ls`) cover every non-archived project.

`--all-projects` on a listing command covers every non-archived project and ignores
`TASKIST_PROJECT` and the current directory; it cannot be combined with `-p`. `export` is
restricted only by its own `-p`. Commands that take a task id (`show`, `edit`, the status
commands, `note`) need no scope: ids are global.

## Global options

- `--json`: print one JSON envelope instead of human text.
- `--by ACTOR`: the actor recorded on created tasks and notes, such as `alan` or
  `agent:web-refactor`.
- `--help`, `--version`: print text to stdout and exit 0.

Names of projects, features and tags use lowercase letters, digits and `-`, start with a letter
or digit, and are at most 64 characters. Titles are one line. Priorities run from 0 (most
urgent) to 3; the default is 2.

## Projects

`tk project add NAME [--path DIR] [--desc TEXT]` registers a project; commands run inside
`DIR` target it. `tk project ls [--all]` lists projects with their open-task counts, archived
ones only with `--all`. `tk project show NAME` shows a project, its features and its task
counts per status. `tk project edit NAME [--name NEW] [--path DIR | --no-path] [--desc TEXT]`
changes it. `tk project archive NAME [--undo]` archives or unarchives it; an archived project
takes no new tasks. `tk project rm NAME [--force]` deletes it; one that has tasks needs
`--force`, which deletes its features, tasks, tags and notes too.

```sh
tk project add web --path . --desc "Storefront"
tk project add api
tk project ls
tk project show web
tk project edit api --desc "Public HTTP API"
tk project add scratch
tk project archive scratch
tk project archive scratch --undo
tk project rm scratch
```

## Tasks

`tk add TITLE [-p P] [-f FEATURE] [--pri N] [--tag TAG]... [--body TEXT|-]` creates a task in
status `todo`; its feature is created when it does not exist. `--body -` reads the body from
stdin.

`tk ls [-p P] [--all-projects] [-f FEATURE] [--status S,S] [--tag TAG] [--all] [--limit N]`
lists open tasks (`todo`, `doing`, `blocked`) grouped by project and feature, ordered by
priority, then age, then id; `--all` adds `done` and `dropped` tasks, `--status` picks
statuses.

`tk show ID` shows a task with all its notes.

`tk edit ID [--title T] [--body TEXT|-] [--pri N] [-f FEATURE | --no-feature] [--tag [+|-]TAG]... [-p P]`
changes a task; at least one option is required. `--tag +x` or `--tag x` adds a tag,
`--tag -x` removes one; `-p` moves the task to another project, its feature moving along by
name.

`tk next [-p P] [--all-projects] [-f FEATURE]` shows the open task to work on next: lowest
priority number, then `doing` before `todo`, then oldest. Blocked tasks are never next.

`tk find QUERY [-p P] [--all-projects] [--all]` finds tasks whose title, body or notes contain
`QUERY`, ignoring case; open tasks only unless `--all`.

```sh
tk add "Fix login redirect" -f auth --pri 1 --tag bug
tk add "Write onboarding docs" -f docs
tk add "Add rate limiting" -p api --pri 0
TASKIST_PROJECT=api tk add "Document error codes" --body "List every code."
tk ls
tk ls --all-projects
tk ls --status todo,doing --tag bug --json
tk show 1
tk edit 1 --title "Fix the login redirect loop" --tag +urgent
printf 'Steps:\n1. sign in with an expired session\n' | tk edit 1 --body -
tk next
tk find redirect --all-projects
```

## Status and notes

Statuses are `todo`, `doing`, `blocked`, `done` and `dropped`; `done` and `dropped` are closed.

| Command | From | To |
|---|---|---|
| `tk start ID...` | `todo`, `blocked` | `doing` |
| `tk block ID REASON` | `todo`, `doing` | `blocked` |
| `tk done ID... [NOTE]` | `todo`, `doing`, `blocked` | `done` |
| `tk drop ID REASON` | `todo`, `doing`, `blocked` | `dropped` |
| `tk reopen ID...` | `doing`, `blocked`, `done`, `dropped` | `todo` |

A command on a task already in its target status is a successful no-op (`changed` is false),
so retries are safe. Every other change, such as `start` on a `done` task, fails with
`invalid_transition`. With several ids nothing changes unless every one may change. The reason
of `block` and `drop` and the note of `done` are stored as notes. `done` takes leading numbers
as ids and at most one note after them; a note that is a number needs `--note TEXT`.
`done` and `drop` set `closed_at`; `reopen` clears it.

`tk note ID TEXT` appends a note to a task in any status.

```sh
tk start 1
tk note 1 "Reproduced with an expired session"
tk block 2 "Waiting for the style guide"
tk note 2 "Asked design for the style guide" --by agent:docs
tk start 2
tk done 1 "Redirect now keeps the target page"
tk drop 4 "Covered by the API reference"
tk reopen 4
tk show 4 --json
```

## Features

`tk feature ls [-p P] [--all-projects]` lists features with their open and total task counts.
`tk feature mv OLD NEW [-p P]` renames a feature; when `NEW` exists, the tasks of `OLD` move
there and `OLD` is deleted.

```sh
tk feature ls
tk feature mv docs documentation
```

## Digest, export and import

`tk brief [-p P] [--all-projects]` prints a markdown digest of open work: per project a heading
with the open count, per feature a list of tasks with id, priority, status and title, and a
separate "Blocked" list with the reasons.

`tk export [--format json|md] [-p P]` prints every project, archived ones included, with every
feature, task (closed ones included), tag and note; `-p` restricts it to one project. The JSON
document is `{version: 1, exported_at, projects: [{name, path, description, archived,
created_at, features: [{name, created_at}], tasks: [{...task fields, notes: [...]}]}]}`; `md`
renders the same content as headings and checkbox lists. An export is a report, not a backup:
there is no command that reads it back.

`tk import FILE|-` creates tasks from a file, or stdin with `-`, holding one JSON object per
line with `title` (required), `project`, `feature`, `pri`, `tags`, `body` and `by`. Blank lines
are skipped; an unknown field is a `usage` error. A line's `project` must exist and overrides
the resolved scope; lines without one use the scope of `-p`, `TASKIST_PROJECT` or the current
directory. The whole file is one transaction: a bad line fails the import, creates nothing,
and the error message begins `line N:`.

```sh
tk brief
tk export --format md
tk export -p web > web-export.json
printf '%s\n' '{"title":"Check the cache headers","feature":"perf","pri":1}' '{"title":"Rotate signing keys","project":"api","tags":["security"]}' > followups.jsonl
tk import followups.jsonl
tk ls --all --all-projects --json
```

## Guide and completions

`tk guide` prints this guide. `tk completions SHELL` prints the completion script for `bash`,
`elvish`, `fish`, `nushell`, `powershell` or `zsh`; any other shell is a `usage` error.

```sh
tk guide --json
tk completions bash > tk.bash
```

## Output

Human text is the default. `--json` or `TASKIST_FORMAT=json` selects JSON:

- Success: exactly one line `{"ok":true,"data":...}` on stdout, nothing on stderr, exit 0.
- Failure: exactly one line `{"ok":false,"error":{"code":"...","message":"..."}}` on stderr,
  nothing on stdout, the exit code of the error code.
- Human failure: `error: <message>` on stderr.
- Argument errors follow the same rules: exit 2, and the JSON envelope when `--json` appears
  among the arguments or `TASKIST_FORMAT=json` is set.

Field names are stable; changes only add fields.

### Task fields

A task is `{id, project, feature, title, body, status, priority, tags, created_at, updated_at,
closed_at, created_by}`:

| Field | Meaning |
|---|---|
| `id` | Global integer id, never reused within a database. |
| `project` | Project name. |
| `feature` | Feature name, or null. |
| `title` | One-line title. |
| `body` | Free text, empty when not given. |
| `status` | `todo`, `doing`, `blocked`, `done` or `dropped`. |
| `priority` | 0 (most urgent) to 3. |
| `tags` | Tag names, sorted. |
| `created_at`, `updated_at` | UTC timestamps. |
| `closed_at` | When the task became `done` or `dropped`, or null. |
| `created_by` | The actor that created it. |

A note is `{id, kind, text, author, created_at}` with `kind` one of `note`, `blocked`, `done`
and `dropped`. A project is `{name, path, description, archived, created_at, open}`; a feature
is `{project, name, open, total}`. A scope is `{project, source}` with the project name or
null and `source` one of `flag`, `env`, `cwd` and `none`.

### Data of each command

| Command | `data` |
|---|---|
| `add`, `edit`, `note` | `{task}` |
| `show` | `{task, notes}` |
| `ls`, `find` | `{scope, tasks}` |
| `next` | `{scope, task}`, `task` null when nothing is open |
| `start`, `block`, `done`, `drop`, `reopen` | `{tasks: [{task, changed}]}` in argument order |
| `project add`, `project edit` | `{project}` |
| `project ls` | `{projects}` |
| `project show` | `{project, features, counts: {todo, doing, blocked, done, dropped}}` |
| `project archive` | `{project, changed}` |
| `project rm` | `{removed, tasks}`: the name and the number of deleted tasks |
| `feature ls` | `{scope, features}` |
| `feature mv` | `{feature, merged, moved}` |
| `brief` | `{scope, text}` |
| `export --format json` | the export document |
| `export --format md`, `guide`, `completions` | `{text}` |
| `import` | `{created}`: the ids of the created tasks in file order |

## Error codes

| Code | Exit | Meaning |
|---|---|---|
| `internal` | 1 | unexpected failure: I/O, SQLite, corrupt data |
| `unsupported_schema` | 1 | database written by a newer version |
| `usage` | 2 | bad arguments, bad values, missing scope, malformed import line |
| `not_found` | 3 | unknown task id, project or feature |
| `conflict` | 4 | duplicate name or path, archived project, non-empty project removal |
| `invalid_transition` | 4 | status change the transition table forbids |

## Exit codes

| Exit | Meaning |
|---|---|
| 0 | success, including a no-op status change |
| 1 | `internal`, `unsupported_schema` |
| 2 | `usage` |
| 3 | `not_found` |
| 4 | `conflict`, `invalid_transition` |

## Concurrency

Several people and agents may run `tk` at once against one database: each command is one
transaction in SQLite WAL mode, and a writer waits up to 5 seconds for another to finish.
