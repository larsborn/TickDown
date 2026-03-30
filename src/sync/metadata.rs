use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::ticket::Ticket;

#[derive(Debug, Clone)]
pub struct SyncMetadata {
    pub provider: String,
    pub repo: String,
    pub remote_id: String,
    pub content_hash: String,
    pub remote_updated: DateTime<Utc>,
    pub comment_count: usize,
}

impl SyncMetadata {
    /// Parse sync metadata from raw frontmatter text.
    /// Returns None if required sync fields are missing.
    pub fn from_frontmatter(fm: &str) -> Option<Self> {
        let mut provider = None;
        let mut repo = None;
        let mut remote_id = None;
        let mut content_hash = None;
        let mut remote_updated = None;
        let mut comment_count = None;

        for line in fm.lines() {
            if let Some((key, val)) = line.split_once(':') {
                let key = key.trim();
                let val = val.trim();
                match key {
                    "sync_provider" => provider = Some(val.to_string()),
                    "sync_repo" => repo = Some(val.to_string()),
                    "sync_issue" => remote_id = Some(val.to_string()),
                    "sync_hash" => content_hash = Some(val.to_string()),
                    "sync_remote_updated" => {
                        remote_updated = DateTime::parse_from_rfc3339(val)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc));
                    }
                    "sync_comment_count" => {
                        comment_count = val.parse().ok();
                    }
                    _ => {}
                }
            }
        }

        Some(SyncMetadata {
            provider: provider?,
            repo: repo?,
            remote_id: remote_id?,
            content_hash: content_hash?,
            remote_updated: remote_updated?,
            comment_count: comment_count?,
        })
    }

    /// Serialize sync metadata to frontmatter lines.
    pub fn to_frontmatter_string(&self) -> String {
        format!(
            "sync_provider: {}\nsync_repo: {}\nsync_issue: {}\nsync_hash: {}\nsync_remote_updated: {}\nsync_comment_count: {}",
            self.provider,
            self.repo,
            self.remote_id,
            self.content_hash,
            self.remote_updated.to_rfc3339(),
            self.comment_count,
        )
    }

    /// Merge sync metadata into existing frontmatter, preserving non-sync fields.
    /// If existing is None, returns just the sync lines.
    pub fn merge_into_frontmatter(&self, existing: Option<&str>) -> String {
        let sync_lines = self.to_frontmatter_string();

        let Some(existing) = existing else {
            return sync_lines;
        };

        // Keep non-sync lines, then append sync lines
        let mut preserved: Vec<&str> = Vec::new();
        for line in existing.lines() {
            let key = line.split_once(':').map(|(k, _)| k.trim()).unwrap_or("");
            if !key.starts_with("sync_") {
                preserved.push(line);
            }
        }

        if preserved.is_empty() {
            sync_lines
        } else {
            format!("{}\n{}", preserved.join("\n"), sync_lines)
        }
    }
}

/// Compute SHA-256 hash of canonical ticket content excluding frontmatter.
pub fn compute_content_hash(ticket: &Ticket, default_status: &str) -> String {
    let content = ticket.content_for_hash(default_status);
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ticket::{Comment, TicketId};
    use chrono::NaiveDate;

    fn sample_metadata() -> SyncMetadata {
        SyncMetadata {
            provider: "github".to_string(),
            repo: "owner/repo".to_string(),
            remote_id: "42".to_string(),
            content_hash: "abc123".to_string(),
            remote_updated: DateTime::parse_from_rfc3339("2026-03-30T14:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            comment_count: 5,
        }
    }

    #[test]
    fn test_from_frontmatter_valid() {
        let fm = "sync_provider: github\nsync_repo: owner/repo\nsync_issue: 42\nsync_hash: abc123\nsync_remote_updated: 2026-03-30T14:00:00+00:00\nsync_comment_count: 5";
        let meta = SyncMetadata::from_frontmatter(fm).unwrap();
        assert_eq!(meta.provider, "github");
        assert_eq!(meta.repo, "owner/repo");
        assert_eq!(meta.remote_id, "42");
        assert_eq!(meta.content_hash, "abc123");
        assert_eq!(meta.comment_count, 5);
    }

    #[test]
    fn test_from_frontmatter_missing_field() {
        let fm = "sync_provider: github\nsync_repo: owner/repo";
        assert!(SyncMetadata::from_frontmatter(fm).is_none());
    }

    #[test]
    fn test_from_frontmatter_with_extra_fields() {
        let fm = "tags: important\nsync_provider: github\nsync_repo: owner/repo\nsync_issue: 1\nsync_hash: h\nsync_remote_updated: 2026-01-01T00:00:00+00:00\nsync_comment_count: 0\npriority: high";
        let meta = SyncMetadata::from_frontmatter(fm).unwrap();
        assert_eq!(meta.provider, "github");
        assert_eq!(meta.remote_id, "1");
    }

    #[test]
    fn test_roundtrip() {
        let meta = sample_metadata();
        let fm = meta.to_frontmatter_string();
        let parsed = SyncMetadata::from_frontmatter(&fm).unwrap();
        assert_eq!(parsed.provider, meta.provider);
        assert_eq!(parsed.repo, meta.repo);
        assert_eq!(parsed.remote_id, meta.remote_id);
        assert_eq!(parsed.content_hash, meta.content_hash);
        assert_eq!(parsed.comment_count, meta.comment_count);
    }

    #[test]
    fn test_merge_into_empty_frontmatter() {
        let meta = sample_metadata();
        let result = meta.merge_into_frontmatter(None);
        assert!(result.contains("sync_provider: github"));
        assert!(result.contains("sync_issue: 42"));
    }

    #[test]
    fn test_merge_preserves_non_sync_fields() {
        let meta = sample_metadata();
        let existing = "tags: important\npriority: high\nsync_hash: old_hash\nsync_provider: old";
        let result = meta.merge_into_frontmatter(Some(existing));
        assert!(result.contains("tags: important"));
        assert!(result.contains("priority: high"));
        assert!(result.contains("sync_hash: abc123"));
        assert!(!result.contains("old_hash"));
        assert!(result.contains("sync_provider: github"));
    }

    #[test]
    fn test_compute_content_hash_consistent() {
        let ticket = Ticket {
            id: TicketId::new("LAW", 1),
            title: "Test".to_string(),
            status: Some("New".to_string()),
            frontmatter: Some("sync_hash: whatever".to_string()),
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        let h1 = compute_content_hash(&ticket, "New");
        let h2 = compute_content_hash(&ticket, "New");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_compute_content_hash_excludes_frontmatter() {
        let t1 = Ticket {
            id: TicketId::new("LAW", 1),
            title: "Test".to_string(),
            status: Some("New".to_string()),
            frontmatter: Some("sync_hash: aaa".to_string()),
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        let t2 = Ticket {
            id: TicketId::new("LAW", 1),
            title: "Test".to_string(),
            status: Some("New".to_string()),
            frontmatter: Some("sync_hash: bbb".to_string()),
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        assert_eq!(
            compute_content_hash(&t1, "New"),
            compute_content_hash(&t2, "New")
        );
    }

    #[test]
    fn test_compute_content_hash_changes_on_content_change() {
        let t1 = Ticket {
            id: TicketId::new("LAW", 1),
            title: "Test".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        let t2 = Ticket {
            id: TicketId::new("LAW", 1),
            title: "Test".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![Comment {
                author: "Lars".to_string(),
                timestamp: Some(
                    NaiveDate::from_ymd_opt(2026, 1, 1)
                        .unwrap()
                        .and_hms_opt(10, 0, 0)
                        .unwrap(),
                ),
                body: "New comment".to_string(),
            }],
            is_closed: false,
        };
        assert_ne!(
            compute_content_hash(&t1, "New"),
            compute_content_hash(&t2, "New")
        );
    }
}
