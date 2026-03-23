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
  show            Pretty-print a ticket
  list (ls)       List tickets
  edit            Open ticket in $EDITOR
  comment         Add a comment to a ticket (normalizes file)
  close (done)    Move ticket to done/
  rename (mv)     Rename a ticket's title
  modify (mod)    Move a ticket to a different project/prefix
  rename-project  Rename an entire project (name and/or prefix)
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
  main.rs          CLI definition (clap derive), entry point
  config.rs        Config loading (.tickdown.toml)
  ticket.rs        Data model (TicketId, Ticket, Comment) + canonical serializer
  parser.rs        Lenient markdown parser for existing files
  store.rs         Filesystem operations (scan, read, write, move to done/)
  commands/
    create.rs         Create new ticket with auto-incremented number
    show.rs           Pretty-print ticket with colored output
    list.rs           List tickets with filtering
    edit.rs           Open in $EDITOR
    comment.rs        Append comment and normalize file
    close.rs          Move to done/
    rename.rs         Rename a ticket's title
    modify.rs         Move a ticket to a different project/prefix
    rename_project.rs Rename an entire project (files + config)
```

## Testing

```
cargo test
```

135 tests across 11 modules (97% line coverage):

- `config.rs` -- project/prefix lookups, resolve_prefix priority, TOML parsing
- `ticket.rs` -- TicketId parsing and rejection, Display, canonical serialization
- `parser.rs` -- filename parsing, ticket content parsing, roundtrip stability
- `store.rs` -- filename sanitization, scan, read, write, move, roundtrip
- `commands/create.rs` -- ticket creation, auto-increment, project/prefix resolution
- `commands/comment.rs` -- comment appending, file normalization
- `commands/close.rs` -- move to done/
- `commands/show.rs` -- display with various formats
- `commands/list.rs` -- filtering by prefix/project, include done
- `commands/rename.rs` -- title changes
- `commands/modify.rs` -- cross-project moves, validation
- `commands/rename_project.rs` -- bulk file rename, config updates, conflict detection
