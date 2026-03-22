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
        assert_eq!(
            sanitize_filename("What: A/B Test?"),
            "What- A-B Test-"
        );
    }
}
