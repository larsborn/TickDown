use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::parser::{parse_filename, parse_ticket};
use crate::ticket::{Ticket, TicketId};

pub struct TicketStore {
    pub root: PathBuf,
    pub done_dir: PathBuf,
}

impl TicketStore {
    pub fn new(notes_dir: &Path) -> Self {
        Self {
            root: notes_dir.to_path_buf(),
            done_dir: notes_dir.join("done"),
        }
    }

    pub fn scan_tickets(&self, include_done: bool) -> Result<Vec<Ticket>> {
        let mut tickets = Vec::new();
        tickets.extend(self.scan_dir(&self.root, false)?);
        if include_done {
            tickets.extend(self.scan_dir(&self.done_dir, true)?);
        }
        tickets.sort_by(|a, b| {
            a.id.prefix
                .cmp(&b.id.prefix)
                .then(a.id.number.cmp(&b.id.number))
        });
        Ok(tickets)
    }

    fn scan_dir(&self, dir: &Path, is_closed: bool) -> Result<Vec<Ticket>> {
        let mut tickets = Vec::new();
        if !dir.exists() {
            return Ok(tickets);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().map(|e| e != "md").unwrap_or(true) {
                continue;
            }
            let stem = path.file_stem().unwrap().to_string_lossy();
            if let Some((id, title)) = parse_filename(&stem) {
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;
                let ticket = parse_ticket(id, &title, &content, is_closed);
                tickets.push(ticket);
            }
        }
        Ok(tickets)
    }

    pub fn read_ticket(&self, id: &TicketId) -> Result<Ticket> {
        if let Some(ticket) = self.find_ticket_in_dir(&self.root, id, false)? {
            return Ok(ticket);
        }
        if let Some(ticket) = self.find_ticket_in_dir(&self.done_dir, id, true)? {
            return Ok(ticket);
        }
        bail!("Ticket {} not found", id)
    }

    fn find_ticket_in_dir(
        &self,
        dir: &Path,
        id: &TicketId,
        is_closed: bool,
    ) -> Result<Option<Ticket>> {
        if !dir.exists() {
            return Ok(None);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().map(|e| e != "md").unwrap_or(true) {
                continue;
            }
            let stem = path.file_stem().unwrap().to_string_lossy();
            if let Some((file_id, title)) = parse_filename(&stem) {
                if file_id.prefix == id.prefix && file_id.number == id.number {
                    let content = std::fs::read_to_string(&path)?;
                    return Ok(Some(parse_ticket(file_id, &title, &content, is_closed)));
                }
            }
        }
        Ok(None)
    }

    pub fn ticket_path(&self, ticket: &Ticket) -> PathBuf {
        let dir = if ticket.is_closed {
            &self.done_dir
        } else {
            &self.root
        };
        let safe_title = sanitize_filename(&ticket.title);
        let filename = if safe_title.is_empty() {
            format!("{}.md", ticket.id)
        } else {
            format!("{} {}.md", ticket.id, safe_title)
        };
        dir.join(filename)
    }

    pub fn find_ticket_file(&self, id: &TicketId) -> Result<PathBuf> {
        if let Some(path) = self.find_file_in_dir(&self.root, id)? {
            return Ok(path);
        }
        if let Some(path) = self.find_file_in_dir(&self.done_dir, id)? {
            return Ok(path);
        }
        bail!("Ticket file for {} not found", id)
    }

    fn find_file_in_dir(&self, dir: &Path, id: &TicketId) -> Result<Option<PathBuf>> {
        if !dir.exists() {
            return Ok(None);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().map(|e| e != "md").unwrap_or(true) {
                continue;
            }
            let stem = path.file_stem().unwrap().to_string_lossy();
            if let Some((file_id, _)) = parse_filename(&stem) {
                if file_id.prefix == id.prefix && file_id.number == id.number {
                    return Ok(Some(path));
                }
            }
        }
        Ok(None)
    }

    pub fn write_ticket(&self, ticket: &Ticket, default_status: &str) -> Result<PathBuf> {
        let new_path = self.ticket_path(ticket);

        // Find and remove old file if it exists (title might have changed)
        if let Ok(old_path) = self.find_ticket_file(&ticket.id) {
            if old_path != new_path {
                std::fs::remove_file(&old_path)?;
            }
        }

        let content = ticket.to_canonical(default_status);
        std::fs::write(&new_path, &content)?;
        Ok(new_path)
    }

    pub fn next_number(&self, prefix: &str) -> Result<u32> {
        let mut max = 0u32;
        for dir in [&self.root, &self.done_dir] {
            if !dir.exists() {
                continue;
            }
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if !path.is_file() || path.extension().map(|e| e != "md").unwrap_or(true) {
                    continue;
                }
                let stem = path.file_stem().unwrap().to_string_lossy();
                if let Some((id, _)) = parse_filename(&stem) {
                    if id.prefix == prefix {
                        max = max.max(id.number);
                    }
                }
            }
        }
        Ok(max + 1)
    }

    pub fn move_to_done(&self, id: &TicketId) -> Result<PathBuf> {
        let src = self
            .find_file_in_dir(&self.root, id)?
            .ok_or_else(|| anyhow::anyhow!("Ticket {} not found in open tickets", id))?;
        std::fs::create_dir_all(&self.done_dir)?;
        let filename = src.file_name().unwrap();
        let dest = self.done_dir.join(filename);
        std::fs::rename(&src, &dest)?;
        Ok(dest)
    }

    pub fn move_from_done(&self, id: &TicketId) -> Result<PathBuf> {
        let src = self
            .find_file_in_dir(&self.done_dir, id)?
            .ok_or_else(|| anyhow::anyhow!("Ticket {} not found in done/", id))?;
        let filename = src.file_name().unwrap();
        let dest = self.root.join(filename);
        std::fs::rename(&src, &dest)?;
        Ok(dest)
    }
}

fn sanitize_filename(title: &str) -> String {
    title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ticket::Comment;
    use chrono::NaiveDate;

    fn make_ticket(prefix: &str, number: u32, title: &str, is_closed: bool) -> Ticket {
        Ticket {
            id: TicketId {
                prefix: prefix.to_string(),
                number,
            },
            title: title.to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![],
            is_closed,
        }
    }

    fn write_file(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).unwrap();
    }

    // --- sanitize_filename ---

    #[test]
    fn test_sanitize_clean_title() {
        assert_eq!(sanitize_filename("Normal Title"), "Normal Title");
    }

    #[test]
    fn test_sanitize_forward_slash() {
        assert_eq!(
            sanitize_filename("Issues / Ideas for later"),
            "Issues - Ideas for later"
        );
    }

    #[test]
    fn test_sanitize_backslash() {
        assert_eq!(sanitize_filename("A\\B"), "A-B");
    }

    #[test]
    fn test_sanitize_all_unsafe_chars() {
        assert_eq!(sanitize_filename(r#"/\:*?"<>|"#), "---------");
    }

    #[test]
    fn test_sanitize_empty() {
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn test_sanitize_preserves_unicode() {
        assert_eq!(sanitize_filename("Löschkonzept"), "Löschkonzept");
    }

    #[test]
    fn test_sanitize_mixed() {
        assert_eq!(sanitize_filename("What: A/B Test?"), "What- A-B Test-");
    }

    // --- TicketStore::new ---

    #[test]
    fn test_store_new() {
        let store = TicketStore::new(Path::new("/notes"));
        assert_eq!(store.root, PathBuf::from("/notes"));
        assert_eq!(store.done_dir, PathBuf::from("/notes/done"));
    }

    // --- ticket_path ---

    #[test]
    fn test_ticket_path_open() {
        let store = TicketStore::new(Path::new("/notes"));
        let ticket = make_ticket("LAW", 2, "My Title", false);
        assert_eq!(store.ticket_path(&ticket), PathBuf::from("/notes/LAW-2 My Title.md"));
    }

    #[test]
    fn test_ticket_path_closed() {
        let store = TicketStore::new(Path::new("/notes"));
        let ticket = make_ticket("LAW", 2, "My Title", true);
        assert_eq!(
            store.ticket_path(&ticket),
            PathBuf::from("/notes/done/LAW-2 My Title.md")
        );
    }

    #[test]
    fn test_ticket_path_empty_title() {
        let store = TicketStore::new(Path::new("/notes"));
        let ticket = make_ticket("IDEA", 1, "", false);
        assert_eq!(store.ticket_path(&ticket), PathBuf::from("/notes/IDEA-1.md"));
    }

    #[test]
    fn test_ticket_path_sanitizes_title() {
        let store = TicketStore::new(Path::new("/notes"));
        let ticket = make_ticket("LAW", 3, "Issues / Ideas", false);
        assert_eq!(
            store.ticket_path(&ticket),
            PathBuf::from("/notes/LAW-3 Issues - Ideas.md")
        );
    }

    // --- scan_tickets ---

    #[test]
    fn test_scan_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());
        let tickets = store.scan_tickets(false).unwrap();
        assert!(tickets.is_empty());
    }

    #[test]
    fn test_scan_nonexistent_dir() {
        let store = TicketStore::new(Path::new("/nonexistent/path"));
        let tickets = store.scan_tickets(false).unwrap();
        assert!(tickets.is_empty());
    }

    #[test]
    fn test_scan_finds_tickets() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "LAW-1 First.md", "# LAW-1 First\n* Status: New\n");
        write_file(dir.path(), "LAW-2 Second.md", "# LAW-2 Second\n* Status: Done\n");
        write_file(dir.path(), "not-a-ticket.txt", "ignored");

        let store = TicketStore::new(dir.path());
        let tickets = store.scan_tickets(false).unwrap();
        assert_eq!(tickets.len(), 2);
        assert_eq!(tickets[0].id.number, 1);
        assert_eq!(tickets[1].id.number, 2);
    }

    #[test]
    fn test_scan_ignores_non_md_files() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "LAW-1 First.md", "# LAW-1 First\n");
        write_file(dir.path(), "LAW-2 Second.txt", "not markdown");
        write_file(dir.path(), "readme.md", "no prefix");

        let store = TicketStore::new(dir.path());
        let tickets = store.scan_tickets(false).unwrap();
        assert_eq!(tickets.len(), 1);
    }

    #[test]
    fn test_scan_sorted_by_prefix_then_number() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "GDN-3 Third.md", "");
        write_file(dir.path(), "GDN-1 First.md", "");
        write_file(dir.path(), "AB-2 Other.md", "");

        let store = TicketStore::new(dir.path());
        let tickets = store.scan_tickets(false).unwrap();
        assert_eq!(tickets.len(), 3);
        assert_eq!(tickets[0].id.prefix, "AB");
        assert_eq!(tickets[1].id.number, 1);
        assert_eq!(tickets[2].id.number, 3);
    }

    #[test]
    fn test_scan_includes_done_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let done_dir = dir.path().join("done");
        std::fs::create_dir(&done_dir).unwrap();
        write_file(dir.path(), "LAW-1 Open.md", "");
        write_file(&done_dir, "LAW-2 Closed.md", "");

        let store = TicketStore::new(dir.path());

        let open_only = store.scan_tickets(false).unwrap();
        assert_eq!(open_only.len(), 1);
        assert!(!open_only[0].is_closed);

        let all = store.scan_tickets(true).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|t| t.is_closed));
    }

    // --- read_ticket ---

    #[test]
    fn test_read_ticket_from_root() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            dir.path(),
            "LAW-1 Test.md",
            "# LAW-1 Test\n* Status: New\n",
        );

        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let ticket = store.read_ticket(&id).unwrap();
        assert_eq!(ticket.title, "Test");
        assert!(!ticket.is_closed);
    }

    #[test]
    fn test_read_ticket_from_done() {
        let dir = tempfile::tempdir().unwrap();
        let done_dir = dir.path().join("done");
        std::fs::create_dir(&done_dir).unwrap();
        write_file(&done_dir, "LAW-5 Closed.md", "# LAW-5 Closed\n");

        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 5,
        };
        let ticket = store.read_ticket(&id).unwrap();
        assert!(ticket.is_closed);
    }

    #[test]
    fn test_read_ticket_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 99,
        };
        assert!(store.read_ticket(&id).is_err());
    }

    #[test]
    fn test_read_ticket_prefers_root_over_done() {
        let dir = tempfile::tempdir().unwrap();
        let done_dir = dir.path().join("done");
        std::fs::create_dir(&done_dir).unwrap();
        write_file(dir.path(), "LAW-1 Open.md", "# LAW-1 Open\n* Status: Open\n");
        write_file(&done_dir, "LAW-1 Closed.md", "# LAW-1 Closed\n* Status: Done\n");

        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let ticket = store.read_ticket(&id).unwrap();
        assert!(!ticket.is_closed);
    }

    // --- find_ticket_file ---

    #[test]
    fn test_find_ticket_file_found() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "LAW-1 Title.md", "");

        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let path = store.find_ticket_file(&id).unwrap();
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "LAW-1 Title.md");
    }

    #[test]
    fn test_find_ticket_file_in_done() {
        let dir = tempfile::tempdir().unwrap();
        let done_dir = dir.path().join("done");
        std::fs::create_dir(&done_dir).unwrap();
        write_file(&done_dir, "LAW-3 Done.md", "");

        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 3,
        };
        let path = store.find_ticket_file(&id).unwrap();
        assert!(path.to_str().unwrap().contains("done"));
    }

    #[test]
    fn test_find_ticket_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 99,
        };
        assert!(store.find_ticket_file(&id).is_err());
    }

    // --- write_ticket ---

    #[test]
    fn test_write_ticket_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());
        let ticket = make_ticket("LAW", 1, "Created", false);

        let path = store.write_ticket(&ticket, "New").unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("# LAW-1 Created\n"));
    }

    #[test]
    fn test_write_ticket_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());

        let ticket = make_ticket("LAW", 1, "Original", false);
        store.write_ticket(&ticket, "New").unwrap();

        let ticket2 = make_ticket("LAW", 1, "Original", false);
        let path = store.write_ticket(&ticket2, "New").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_write_ticket_renames_on_title_change() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());

        let ticket = make_ticket("LAW", 1, "Old Title", false);
        let old_path = store.write_ticket(&ticket, "New").unwrap();
        assert!(old_path.exists());

        let ticket2 = make_ticket("LAW", 1, "New Title", false);
        let new_path = store.write_ticket(&ticket2, "New").unwrap();
        assert!(new_path.exists());
        assert!(!old_path.exists());
    }

    #[test]
    fn test_write_ticket_with_comments() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());
        let ts = NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let ticket = Ticket {
            id: TicketId {
                prefix: "LAW".to_string(),
                number: 1,
            },
            title: "Test".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![Comment {
                author: "Author".to_string(),
                timestamp: Some(ts),
                body: "Hello.".to_string(),
            }],
            is_closed: false,
        };

        let path = store.write_ticket(&ticket, "New").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("## Author (2026-01-01 10:00)"));
        assert!(content.contains("Hello."));
    }

    // --- next_number ---

    #[test]
    fn test_next_number_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());
        assert_eq!(store.next_number("LAW").unwrap(), 1);
    }

    #[test]
    fn test_next_number_increments() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "LAW-3 Three.md", "");
        write_file(dir.path(), "LAW-1 One.md", "");

        let store = TicketStore::new(dir.path());
        assert_eq!(store.next_number("LAW").unwrap(), 4);
    }

    #[test]
    fn test_next_number_includes_done() {
        let dir = tempfile::tempdir().unwrap();
        let done_dir = dir.path().join("done");
        std::fs::create_dir(&done_dir).unwrap();
        write_file(dir.path(), "LAW-2 Open.md", "");
        write_file(&done_dir, "LAW-5 Closed.md", "");

        let store = TicketStore::new(dir.path());
        assert_eq!(store.next_number("LAW").unwrap(), 6);
    }

    #[test]
    fn test_next_number_ignores_other_prefixes() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "LAW-10 Law.md", "");
        write_file(dir.path(), "GDN-20 Garden.md", "");

        let store = TicketStore::new(dir.path());
        assert_eq!(store.next_number("LAW").unwrap(), 11);
        assert_eq!(store.next_number("GDN").unwrap(), 21);
        assert_eq!(store.next_number("NEW").unwrap(), 1);
    }

    // --- move_to_done ---

    #[test]
    fn test_move_to_done() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            dir.path(),
            "LAW-1 Title.md",
            "# LAW-1 Title\n* Status: New\n",
        );

        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let dest = store.move_to_done(&id).unwrap();
        assert!(dest.exists());
        assert!(dest.to_str().unwrap().contains("done"));
        assert!(!dir.path().join("LAW-1 Title.md").exists());
    }

    #[test]
    fn test_move_to_done_creates_done_dir() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "LAW-1 T.md", "");

        let store = TicketStore::new(dir.path());
        assert!(!store.done_dir.exists());

        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        store.move_to_done(&id).unwrap();
        assert!(store.done_dir.exists());
    }

    #[test]
    fn test_move_to_done_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 99,
        };
        assert!(store.move_to_done(&id).is_err());
    }

    #[test]
    fn test_move_from_done() {
        let dir = tempfile::tempdir().unwrap();
        let done = dir.path().join("done");
        std::fs::create_dir(&done).unwrap();
        write_file(&done, "LAW-1 T.md", "# LAW-1 T\n");

        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let dest = store.move_from_done(&id).unwrap();
        assert_eq!(dest, dir.path().join("LAW-1 T.md"));
        assert!(dest.exists());
        assert!(!done.join("LAW-1 T.md").exists());
    }

    #[test]
    fn test_move_from_done_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "LAW".into(),
            number: 99,
        };
        assert!(store.move_from_done(&id).is_err());
    }

    // --- roundtrip: write then read ---

    #[test]
    fn test_write_then_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());

        let ts = NaiveDate::from_ymd_opt(2026, 3, 21)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap();
        let ticket = Ticket {
            id: TicketId {
                prefix: "LAW".to_string(),
                number: 7,
            },
            title: "Roundtrip".to_string(),
            status: Some("In Progress".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![
                Comment {
                    author: "Alice".to_string(),
                    timestamp: Some(ts),
                    body: "First.".to_string(),
                },
                Comment {
                    author: "Bob".to_string(),
                    timestamp: None,
                    body: "Second.".to_string(),
                },
            ],
            is_closed: false,
        };

        store.write_ticket(&ticket, "New").unwrap();

        let id = TicketId {
            prefix: "LAW".into(),
            number: 7,
        };
        let read_back = store.read_ticket(&id).unwrap();
        assert_eq!(read_back.title, "Roundtrip");
        assert_eq!(read_back.status.as_deref(), Some("In Progress"));
        assert_eq!(read_back.comments.len(), 2);
        assert_eq!(read_back.comments[0].author, "Alice");
        assert_eq!(read_back.comments[0].body, "First.");
        assert_eq!(read_back.comments[1].author, "Bob");
        assert_eq!(read_back.comments[1].body, "Second.");
    }

    #[test]
    fn test_write_then_read_roundtrip_with_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let store = TicketStore::new(dir.path());

        let ticket = Ticket {
            id: TicketId {
                prefix: "LAW".to_string(),
                number: 1,
            },
            title: "Synced".to_string(),
            status: Some("New".to_string()),
            frontmatter: Some("source: github\nrepo: https://example.com".to_string()),
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };

        store.write_ticket(&ticket, "New").unwrap();

        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let read_back = store.read_ticket(&id).unwrap();
        assert_eq!(
            read_back.frontmatter.as_deref(),
            Some("source: github\nrepo: https://example.com")
        );
        assert_eq!(read_back.title, "Synced");
    }
}
