# TickDown

[![CI](https://github.com/larsborn/TickDown/actions/workflows/ci.yml/badge.svg)](https://github.com/larsborn/TickDown/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/larsborn/TickDown/graph/badge.svg)](https://codecov.io/gh/larsborn/TickDown)

A command-line tool for managing markdown-based tickets. Tickets are plain
`.md` files stored in a directory, with a `done/` subdirectory for closed
tickets. TickDown parses existing files leniently (handling format variations
that accumulate over time) but always writes back in a clean canonical format.

## Installation

Download the latest pre-compiled binary from the
[GitHub Releases](https://github.com/larsborn/TickDown/releases) page. Binaries
are available for Windows (x86_64), Linux (x86_64), and macOS (x86_64). Place
the binary somewhere on your `PATH`.

### Building from source

Alternatively, build it yourself with a Rust toolchain:

```
cargo build --release
```

The binary is at `target/release/td` (or `td.exe` on Windows).

## Configuration

TickDown looks for a `.tickdown.toml` config file using this resolution order:

1. `--config` flag
2. `TICKDOWN_CONFIG` environment variable
3. `.tickdown.toml` in the current directory or any ancestor

Example config:

```toml
default_author = "Your Name"
notes_dir = "C:\\Path\\To\\Nextcloud\\TickDown"

[statuses]
values = ["New", "In Progress", "Waiting For", "Done"]
default = "New"

[projects]
TickDown = "TD"
Piano = "PNO"
```

The `[projects]` table maps human-readable project names to ticket ID prefixes.
Only projects you want to create tickets for need to be listed here -- all
prefixes are recognized when listing or showing tickets.

## Commands

```
Usage: td [OPTIONS] <COMMAND>

Commands:
  create          Create a new ticket
  create-project  Create a new project in the config
  show            Pretty-print a ticket
  list (ls)       List tickets
  edit            Open ticket in $EDITOR
  comment         Add a comment to a ticket (normalizes file)
  close (done)    Move ticket to done/
  rename (mv)     Rename a ticket's title
  modify (mod)    Move a ticket to a different project/prefix
  rename-project  Rename an entire project (name and/or prefix)
  sync            Sync with remote issue trackers
  help            Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>
  -h, --help             Print help
```

## Usage Examples

### List open tickets

```
$ td list --prefix TD
       TD-1            Open  Come up with at least one Visualization
       TD-2     In Progress  Command Line Client Prototype
       TD-3     Waiting For  Issues / Ideas for later
       TD-4     Waiting For  Data
```

Use `--all` to include closed tickets from `done/`. Use `--project` to filter
by project name instead of prefix.

### Create a ticket

```
$ td create TickDown "Implement search command"
Created TD-5 at C:\...\TickDown\TD-5 Implement search command.md
```

You can use either the project name or the prefix directly:

```
$ td create TD "Implement search command"
Created TD-5 at C:\...\TickDown\TD-5 Implement search command.md
```

The number is auto-incremented by scanning existing tickets (both open and
closed) for the highest number with that prefix.

### Create a project

```
$ td create-project Piano PNO
Created project Piano (PNO)
```

Adds a new project to `.tickdown.toml`. The prefix must start with an uppercase
letter and contain only uppercase letters and digits. Rejects duplicate names
or prefixes.

### Add a comment

```
$ td comment TD-2 "Started working on the parser module."
Comment added to TD-2 (C:\...\TD-2 Command Line Client Prototype.md)
```

Adding a comment normalizes the file to canonical format: the heading is
corrected to match the filename, a status line is ensured, and comment headers
get a consistent `## Author (YYYY-MM-DD HH:MM)` format.

### Edit a ticket

```
$ td edit TD-2
```

Opens the ticket file in `$EDITOR` (falls back to `notepad` on Windows). This
is a read-only operation -- it does not normalize the file.

### Close a ticket

```
$ td close TD-12
Closed TD-12 -> D:\...\done\TD-12 Implement search command.md
```

Moves the ticket file into the `done/` subdirectory.

### Rename a ticket

```
$ td rename TD-2 "Better title"
Renamed TD-2 -> Better title (C:\...\TD-2 Better title.md)
```

Also available as `td mv`.

### Move a ticket to another project

```
$ td modify TD-2 --project Piano
Moved TD-2 -> PNO-4 (C:\...\PNO-4 Better title.md)
```

Accepts `--project` (by name) or `--prefix` (directly). The ticket gets the
next available number in the target prefix. Also available as `td mod`.

### Rename an entire project

```
$ td rename-project TickDown --prefix NEW
  TD-1 -> NEW-1
  TD-2 -> NEW-2
  TD-3 -> NEW-3
Changed prefix for TickDown: TD -> NEW
```

Renames all ticket files (including closed ones in `done/`) and updates the
config. You can also change just the project name:

```
$ td rename-project TickDown --name "My Project"
Renamed project TickDown -> My Project (prefix TD unchanged)
```

Or both at once with `--name` and `--prefix` together.

### Sync with GitHub

TickDown can sync tickets bidirectionally with GitHub Issues using the
[GitHub CLI](https://cli.github.com/) (`gh`). The sync architecture is
provider-agnostic — Jira and other integrations can be added in the future.

**Prerequisites:** Install the `gh` CLI and authenticate with `gh auth login`.

#### Set up sync for a project

```
$ td sync init github larsborn/TickDown --project TickDown
Configured sync: TickDown (TD) <-> github (larsborn/TickDown)
Fetching issues from larsborn/TickDown ...
Found 12 issues. Downloading...
  [1/12] Downloading #1 "First issue"...
  Created TD-1 <- remote #1 "First issue"
  ...
Downloaded 12 issues.
```

This adds a `[[sync]]` entry to your `.tickdown.toml`:

```toml
[[sync]]
project = "TickDown"
provider = "github"
repo = "larsborn/TickDown"
```

Each downloaded ticket gets YAML frontmatter with sync metadata:

```markdown
---
sync_provider: github
sync_repo: larsborn/TickDown
sync_issue: 42
sync_hash: a1b2c3...
sync_remote_updated: 2026-03-30T14:00:00+00:00
sync_comment_count: 5
---
# TD-42 Feature request
* Status: New

## requester (2026-03-20 10:00)
Please add this feature.

## maintainer (2026-03-21 09:00)
Good idea!
```

The GitHub issue body becomes the first comment. The preamble is free for your
own local notes.

#### Run sync

```
$ td sync run TickDown
Syncing TickDown (larsborn/TickDown)...
  Updated TD-3 <- remote #3 "Bug report"
  Pushed 1 comment(s) for TD-7 -> remote #7
  CONFLICT: TD-12 (remote #12) — both local and remote changed, skipping
  Pulled: 1
  Pushed (comments): 1
  Conflicts (skipped): 1
  Unchanged: 9
```

Omit the project name to sync all configured projects. The sync detects changes
by comparing a SHA-256 hash of the ticket content with the stored `sync_hash`.

**What syncs:**

- **Pull**: New remote issues are downloaded. Updated remote issues overwrite
  the local title and comments (preamble is preserved). Closed/reopened state
  is reflected by moving files to/from `done/`.
- **Push**: New comments added locally are pushed to GitHub. Tickets created
  locally in a synced project are pushed as new GitHub issues.
- **Conflicts**: If both local and remote changed since last sync, the ticket
  is skipped with a warning.

**Current limitations** (planned for future):

- Title, body, and status changes are not pushed back to GitHub (only new
  comments and new issues are pushed).
- Maximum 1000 issues per repository.

#### Check sync status

```
$ td sync status TickDown
Project: TickDown (TD) -> github (larsborn/TickDown)
  TD-7 (remote #7) — DIRTY (local changes)
  TD-15 — UNSYNCED (will be pushed on next sync)
  Total: 12 synced (1 dirty), 1 unsynced
```

This is a read-only operation that does not contact GitHub.

## Ticket Format

Tickets are markdown files named `PREFIX-NUMBER Title.md`.

Canonical format:

```markdown
# TD-12 Implement search command
* Status: New

## Lars Wallenborn (2026-03-21 14:30)
First comment body here.

## Lars Wallenborn (2026-03-21 15:00)
Second comment with more details.
```

Tickets may optionally have YAML frontmatter (delimited by `---`) at the top
of the file. This is used by the sync feature to store metadata but can also
hold arbitrary key-value pairs. Frontmatter is preserved through all
normalize-on-touch cycles.

### Normalize-on-touch

The `create` and `comment` commands write files in canonical format. When
`comment` reads an existing file with format variations, it normalizes them:

- The heading is rewritten to `# PREFIX-NUMBER Title` (removing `[Project]`
  tags, correcting any ID mismatch with the filename)
- A status line is always present (using the configured default if missing)
- Comment timestamps without a time component get `00:00` appended
- Leading whitespace on `## ` lines is removed

The `show`, `list`, and `edit` commands are read-only and never modify files.
The `close` command only moves the file.

### Parser leniency

The parser handles these variations found in real-world ticket files:

- Filename separators: space (`TD-2 Title.md`), dash (`TD-4-Title.md`)
- Missing titles in filenames (`BLK-1.md`)
- Zero-padded numbers (`IDEA-001.md`)
- Double spaces in filenames (`LMK-1  VB.md`)
- `[Project]` tags in headings (`# TD-2 [TickDown] Title`)
- Missing `#` headings
- Missing status lines
- Heading IDs that don't match the filename (filename wins)
- Comment dates with or without time (`2020-06-13` vs `2020-06-13 14:30`)
- Leading whitespace on `##` comment headers
- Multi-author comments (`Author1 / Author2`)

## Project Structure

```
src/
  main.rs            CLI definition (clap derive), entry point
  config.rs          Config loading (.tickdown.toml)
  ticket.rs          Data model (TicketId, Ticket, Comment) + canonical serializer
  parser.rs          Lenient markdown parser for existing files
  store.rs           Filesystem operations (scan, read, write, move to/from done/)
  sync/
    mod.rs           SyncProvider trait and shared types
    metadata.rs      Sync metadata in frontmatter, SHA-256 content hashing
    github.rs        GitHub provider (gh CLI wrapper + JSON parsing)
    orchestrator.rs  Core sync algorithm (pull, push, conflict detection)
  commands/
    create.rs         Create new ticket with auto-incremented number
    create_project.rs Create a new project in the config
    show.rs           Pretty-print ticket with colored output
    list.rs           List tickets with filtering
    edit.rs           Open in $EDITOR
    comment.rs        Append comment and normalize file
    close.rs          Move to done/
    rename.rs         Rename a ticket's title
    modify.rs         Move a ticket to a different project/prefix
    rename_project.rs Rename an entire project (files + config)
    sync.rs           Sync init, run, and status commands
```

## Testing

```
cargo test
```

225 tests across 16 modules, ~94% line coverage:

- `config.rs` -- project/prefix lookups, resolve_prefix priority, TOML parsing
  (with and without `[[sync]]`)
- `ticket.rs` -- TicketId parsing and rejection, Display, canonical serialization
- `parser.rs` -- filename parsing, ticket content parsing, frontmatter extraction,
  roundtrip stability
- `store.rs` -- filename sanitization, scan, read, write, move to/from done,
  roundtrip
- `sync/metadata.rs` -- sync metadata parse/write, content hashing
- `sync/github.rs` -- JSON parsing for issue lists and details, datetime error
  paths, unknown-state handling
- `sync/orchestrator.rs` -- pull, push, conflict detection, state transitions
  (open/closed), hash-only updates, foreign-provider metadata, `init_sync`,
  `show_status`, `append_sync_to_config` (MockProvider)
- `commands/sync.rs` -- init validation (unknown provider/project, duplicates),
  `resolve_sync_configs` filtering, provider factory
- `commands/create.rs` -- ticket creation, auto-increment, project/prefix resolution
- `commands/create_project.rs` -- project creation, name/prefix validation and conflicts
- `commands/comment.rs` -- comment appending, file normalization
- `commands/close.rs` -- move to done/
- `commands/show.rs` -- display with various formats
- `commands/list.rs` -- filtering by prefix/project, include done
- `commands/rename.rs` -- title changes
- `commands/modify.rs` -- cross-project moves, validation
- `commands/rename_project.rs` -- bulk file rename, config updates, conflict detection
