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

Always run both `cargo test` and `cargo build --release` before considering
work complete. The release build catches warnings and optimizations that the
debug build may not.

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
  that normalize on touch. `show`, `list`, `search`, `edit` are read-only.
  `close` just moves files. `rename` changes titles, `modify` moves tickets
  between projects, `rename_project` renames an entire project (files +
  config). `create_project` adds a new project to the config file.
  `search` does case-insensitive substring matching across title, preamble
  and comment bodies and reuses `list::print_ticket_row` for output.
- `sync/` -- Two-way sync with external issue trackers. `mod.rs` defines the
  `SyncProvider` trait and shared types (`RemoteIssue`, `RemoteComment`).
  `github.rs` implements the GitHub provider via the `gh` CLI (JSON parsing).
  `metadata.rs` handles `sync_*` frontmatter fields and SHA-256 content hashing
  for dirty detection. `orchestrator.rs` contains the core sync algorithm
  (pull, push-comments, push-new, conflict detection).
- `commands/sync.rs` -- CLI handler for `sync init`, `sync run`, `sync status`.
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
- **Frontmatter preservation**: Optional YAML frontmatter (`---` delimited) at
  the top of a ticket file is parsed into `frontmatter: Option<String>` and
  preserved through normalize cycles. Stored as raw text (no YAML parsing
  dependency). Intended for sync-tool metadata (GitHub repo, Jira instance,
  etc.). `create` sets `frontmatter: None`; sync tools add it later. The parser
  is lenient: unclosed `---` is treated as no frontmatter.
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
- `search` -> `s`
- `close` -> `done`
- `rename` -> `mv`
- `modify` -> `mod`

Running `td` with no subcommand defaults to `td list` (open tickets only).

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

240 tests across 17 modules. Run with `cargo test`.
Overall coverage ~94% (regions/lines/functions).
Use `cargo llvm-cov --summary-only` for per-file coverage. Tests cover:
- `ticket.rs` -- TicketId parsing/rejection, Display, all canonical serialization
  variants (empty title, default status, preamble, comments with/without
  timestamps, empty body, multiple comments)
- `config.rs` -- project/prefix lookups, resolve_prefix priority edge case
  (project name matching another prefix), TOML deserialization (with and without
  `[[sync]]`), load from file, load from env var, invalid/missing config,
  ancestor directory search
- `parser.rs` -- filename formats (standard, dash, no title, zero-padded,
  double space, lowercase rejection), ticket parsing (standard, no heading,
  date without time, ID mismatch, multiple comments, leading whitespace,
  multi-author, no date, is_closed, missing status), frontmatter extraction
  (present, absent, unclosed, empty), frontmatter roundtrip stability
- `store.rs` -- filename sanitization, scan, read, write, move (to/from done),
  next_number, roundtrip
- `commands/*` -- integration tests for all commands: create (auto-increment,
  project/prefix resolution), comment (append, normalize), close (move to done),
  show (various formats, preamble, closed), list (filtering, empty, done),
  rename (title change, preserves content), modify (cross-project move,
  validation), rename_project (bulk rename, config update, conflict detection)
- `sync/metadata.rs` -- SyncMetadata parse/write roundtrip, merge preserves
  non-sync fields, content hash consistency/exclusion
- `sync/github.rs` -- JSON parsing for issue list, issue detail, empty/no
  comments, invalid JSON, datetime parse errors (invalid/missing fields),
  unknown state treated as closed, list item error propagation
- `sync/orchestrator.rs` -- pull-new, pull-with-comments, push-new,
  unchanged-no-action, comment formatting, conflict detection, pull updates
  on remote dirty, local-dirty hash-only update, push comments on synced
  ticket, open/closed state transitions, remote deleted warning, foreign
  provider metadata as unsynced, `rebaseline_local` (no new comments,
  pushes new comments, post-rebaseline sync is clean), `show_status`,
  `init_sync`, `append_sync_to_config` (all with MockProvider)
- `commands/sync.rs` -- init validation (unknown provider/project, duplicate
  config), `resolve_sync_configs` filtering and error paths, provider factory,
  `resolve` validation (invalid id, unknown ticket, unsynced ticket, no
  matching `[[sync]]` config)
- `commands/search.rs` -- empty dir, match in title/preamble/comment body,
  case-insensitive matching, no-match path, closed-excluded-by-default,
  `--all` includes done

## Sync feature

Two-way sync with GitHub Issues via `gh` CLI. Generic `SyncProvider` trait
for future Jira/Redmine support.

- `td sync init github owner/repo --project Name` -- configure and initial pull
- `td sync run [project]` -- bidirectional sync (all projects if omitted)
- `td sync status [project]` -- show dirty state (read-only, no remote fetch)
- `td sync resolve <id> --pull|--keep-local [-y]` -- force-resolve a conflict
  on a single ticket. `--pull` overwrites local with remote. `--keep-local`
  pushes any new local comments and re-points sync metadata at the current
  remote without merging remote-side changes. Mutually exclusive flags, one
  required. Prompts for confirmation unless `-y` is given.

**Content mapping**: GitHub issue body → first TickDown Comment (preamble stays
free for local notes). GitHub comments → subsequent Comments.

**Dirty detection**: SHA-256 hash of canonical content (excluding frontmatter)
stored as `sync_hash` in frontmatter. Hash mismatch = locally dirty. Remote
`updatedAt` comparison = remotely dirty. Both dirty = conflict (skip + warn).

**Push scope (current)**: Only new comments are pushed. Title/body/status push
is deferred. New local tickets in synced projects auto-push as new issues.

**Conflict resolution**: When `sync_project` reports `CONFLICT` (both local
and remote dirty), the ticket is skipped — neither side moves. Use
`td sync resolve <id> --pull` (remote wins) or `--keep-local` (re-baseline
to current local + push new comments) to break the deadlock. The
`rebaseline_local` helper in `orchestrator.rs` is shared between the
normal `local_dirty && !remote_dirty` sync path and `--keep-local`.

**Config**: `[[sync]]` array in `.tickdown.toml`:
```toml
[[sync]]
project = "MyProject"
provider = "github"
repo = "owner/repo"
```

## Documentation

Always keep `CLAUDE.md` and `README.md` up-to-date when making changes. This
includes: new commands, changed behavior, new config options, updated test
counts, and project structure changes. Both files should reflect the current
state of the codebase.

## Dependencies

clap 4 (derive), serde + toml, chrono, anyhow, regex, colored, serde_json,
sha2. No async, no database. Sync uses `gh` CLI for GitHub API.
