# TickDown - Development Notes

## What this is

A Rust CLI tool (`td`) that manages markdown-based tickets stored as flat files
in a Nextcloud-synced directory. The ticket directory lives at
`D:\Sync\Nextcloud\Doc\Notes\TickDown` with ~70 existing tickets and a `done/`
subdirectory for closed ones.

## Build and test

```
cargo build --release
cargo test
```

The binary is `td.exe`. Config resolution: `--config` flag > `TICKDOWN_CONFIG`
env var > `.tickdown.toml` in cwd or ancestors.

The production config file is at:
`D:\Sync\Nextcloud\Doc\Notes\TickDown\.tickdown.toml`

## Architecture

- `config.rs` -- Loads `.tickdown.toml` (serde + toml crate). Projects map
  human names to prefixes (e.g., "TickDown" -> "LAW").
- `ticket.rs` -- `TicketId`, `Ticket`, `Comment` structs. `Ticket::to_canonical()`
  serializes to the normalized markdown format.
- `parser.rs` -- The most complex module. Leniently parses existing ticket files
  that have accumulated format variations over years. Key design rule: filename
  is authoritative for ticket identity; heading is corrected on normalize.
  Has unit tests covering all known format variations.
- `store.rs` -- Filesystem layer. Scans directories, reads/writes tickets,
  handles file renaming when titles change, auto-increments ticket numbers.
- `commands/` -- One file per command. `create` and `comment` are write commands
  that normalize on touch. `show`, `list`, `edit` are read-only. `close` just
  moves files. `rename` changes titles, `modify` moves tickets between projects,
  `rename_project` renames an entire project (files + config).
- `main.rs` -- Clap derive CLI. Also enables Windows ANSI terminal support for
  colored output in cmd.exe.

## Key design decisions

- **Normalize-on-touch**: `create`, `comment`, `rename`, and `modify` rewrite
  files in canonical format. Read-only commands (`show`, `list`, `edit`) never
  modify. `close` only moves. `rename-project` does simple prefix string
  replacement without normalization to avoid disrupting file content.
- **Filename is authoritative**: The ticket ID comes from the filename, not the
  heading. If they disagree, the heading is corrected on normalize.
- **Preamble preservation**: Content between the heading/status and the first
  `## ` comment header is stored as a `preamble` field and preserved through
  normalize cycles.
- **No git integration**: The notes directory is synced via Nextcloud, not git.
- **Windows-first**: ANSI color support is explicitly enabled for cmd.exe.
  Editor fallback is `notepad`.

## Known format variations in the wild

These are real patterns found in the ~70 existing tickets. The parser handles
all of them. See `parser.rs` tests for examples:

- `[Project]` tags in headings (e.g., `# LAW-2 [TickDown] Title`)
- Leading whitespace on `## ` comment lines
- Heading ID mismatching filename ID (LAW-4 file has LAW-3 in heading)
- Zero-padded numbers (IDEA-001.md)
- Dash-separated filenames (LAW-4-Title.md)
- Files with no heading at all (IDEA-001.md)
- Files with no ticket structure (BOELKE-1.md is a website content dump)
- Dates without time components
- Double spaces in filenames

## Command aliases

- `list` -> `ls`
- `close` -> `done`
- `rename` -> `mv`
- `modify` -> `mod`

## Filename sanitization

Titles containing path-unsafe characters (`/ \ : * ? " < > |`) are sanitized
when constructing filenames (replaced with `-`). This prevents issues with
titles like "Issues / Ideas" creating subdirectory paths.

## Config file modification

`rename-project` modifies `.tickdown.toml` by parsing it as `toml::Value`,
updating the `[projects]` table, and writing back with `toml::to_string_pretty`.
This may reformat the file slightly (section order, array style) but preserves
all values.

## Tests

52 unit tests across 4 modules. Run with `cargo test`. Tests cover:
- `ticket.rs` -- TicketId parsing/rejection, Display, all canonical serialization
  variants (empty title, default status, preamble, comments with/without
  timestamps, empty body, multiple comments)
- `config.rs` -- project/prefix lookups, resolve_prefix priority edge case
  (project name matching another prefix), TOML deserialization
- `parser.rs` -- filename formats (standard, dash, no title, zero-padded,
  double space, lowercase rejection), ticket parsing (standard, no heading,
  date without time, ID mismatch, multiple comments, leading whitespace,
  multi-author, no date, is_closed, missing status), roundtrip stability
- `store.rs` -- filename sanitization for all path-unsafe characters, unicode
  preservation

## Dependencies

clap 4 (derive), serde + toml, chrono, anyhow, regex, colored. No async, no
database, no network. Intentionally minimal.
