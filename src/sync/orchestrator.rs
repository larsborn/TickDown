use std::collections::HashMap;

use anyhow::{bail, Context, Result};

use crate::config::{Config, SyncConfig};
use crate::store::TicketStore;
use crate::sync::metadata::{compute_content_hash, SyncMetadata};
use crate::sync::{RemoteIssue, RemoteState, SyncProvider};
use crate::ticket::{Comment, Ticket, TicketId};

#[derive(Debug, Default)]
pub struct SyncSummary {
    pub pulled: usize,
    pub pushed_new: usize,
    pub pushed_comments: usize,
    pub conflicts: usize,
    pub unchanged: usize,
    pub errors: Vec<String>,
}

impl SyncSummary {
    pub fn print(&self) {
        if self.pulled > 0 {
            println!("  Pulled: {}", self.pulled);
        }
        if self.pushed_new > 0 {
            println!("  Pushed (new issues): {}", self.pushed_new);
        }
        if self.pushed_comments > 0 {
            println!("  Pushed (comments): {}", self.pushed_comments);
        }
        if self.conflicts > 0 {
            println!("  Conflicts (skipped): {}", self.conflicts);
        }
        if self.unchanged > 0 {
            println!("  Unchanged: {}", self.unchanged);
        }
        for err in &self.errors {
            eprintln!("  Error: {}", err);
        }
    }
}

pub fn sync_project(
    config: &Config,
    sync_config: &SyncConfig,
    provider: &dyn SyncProvider,
) -> Result<SyncSummary> {
    let prefix = config.resolve_prefix(&sync_config.project).ok_or_else(|| {
        anyhow::anyhow!("Unknown project: {}", sync_config.project)
    })?;
    let store = TicketStore::new(&config.notes_dir);
    let default_status = &config.statuses.default;
    let mut summary = SyncSummary::default();

    // Gather local tickets for this prefix
    let all_tickets = store.scan_tickets(true)?;
    let project_tickets: Vec<Ticket> = all_tickets
        .into_iter()
        .filter(|t| t.id.prefix == prefix)
        .collect();

    // Partition into synced (has metadata) and unsynced
    let mut local_synced: HashMap<String, (Ticket, SyncMetadata)> = HashMap::new();
    let mut local_unsynced: Vec<Ticket> = Vec::new();

    for ticket in project_tickets {
        if let Some(ref fm) = ticket.frontmatter {
            if let Some(meta) = SyncMetadata::from_frontmatter(fm) {
                if meta.provider == provider.name() && meta.repo == sync_config.repo {
                    local_synced.insert(meta.remote_id.clone(), (ticket, meta));
                    continue;
                }
            }
        }
        local_unsynced.push(ticket);
    }

    // Fetch remote issues (lightweight, no comments)
    let remote_issues = provider.list_issues()?;
    let mut remote_map: HashMap<String, RemoteIssue> = HashMap::new();
    for issue in remote_issues {
        remote_map.insert(issue.remote_id.clone(), issue);
    }

    // Match local synced ↔ remote
    for (remote_id, (ticket, metadata)) in &local_synced {
        let current_hash = compute_content_hash(ticket, default_status);
        let local_dirty = current_hash != metadata.content_hash;

        if let Some(remote) = remote_map.remove(remote_id) {
            let remote_dirty = remote.updated_at > metadata.remote_updated;

            if local_dirty && remote_dirty {
                eprintln!(
                    "  CONFLICT: {} (remote #{}) — both local and remote changed, skipping",
                    ticket.id, remote_id
                );
                summary.conflicts += 1;
            } else if local_dirty {
                match push_comments(
                    &store,
                    ticket,
                    &metadata,
                    provider,
                    sync_config,
                    default_status,
                ) {
                    Ok(()) => summary.pushed_comments += 1,
                    Err(e) => summary.errors.push(format!("{}: {}", ticket.id, e)),
                }
            } else if remote_dirty {
                match pull_issue(
                    &store,
                    Some(ticket),
                    &remote,
                    &prefix,
                    provider,
                    sync_config,
                    default_status,
                ) {
                    Ok(()) => summary.pulled += 1,
                    Err(e) => summary.errors.push(format!("{}: {}", ticket.id, e)),
                }
            } else {
                summary.unchanged += 1;
            }
        } else {
            eprintln!(
                "  WARNING: {} (remote #{}) — remote issue no longer exists, skipping",
                ticket.id, remote_id
            );
        }
    }

    // Remote-only issues → pull new
    for (remote_id, remote) in &remote_map {
        if local_synced.contains_key(remote_id) {
            continue;
        }
        match pull_issue(
            &store,
            None,
            remote,
            &prefix,
            provider,
            sync_config,
            default_status,
        ) {
            Ok(()) => summary.pulled += 1,
            Err(e) => summary.errors.push(format!("remote #{}: {}", remote_id, e)),
        }
    }

    // Local unsynced → push new
    for ticket in &local_unsynced {
        match push_new_issue(&store, ticket, provider, sync_config, default_status) {
            Ok(()) => summary.pushed_new += 1,
            Err(e) => summary.errors.push(format!("{}: {}", ticket.id, e)),
        }
    }

    Ok(summary)
}

/// Pull a remote issue into a local ticket.
/// If `existing` is Some, updates that ticket. Otherwise creates a new one.
fn pull_issue(
    store: &TicketStore,
    existing: Option<&Ticket>,
    remote_summary: &RemoteIssue,
    prefix: &str,
    provider: &dyn SyncProvider,
    sync_config: &SyncConfig,
    default_status: &str,
) -> Result<()> {
    // Fetch full issue with comments
    let full = provider.fetch_issue(&remote_summary.remote_id)?;

    let (id, was_closed) = if let Some(t) = existing {
        (t.id.clone(), t.is_closed)
    } else {
        let number = store.next_number(prefix)?;
        let id = TicketId {
            prefix: prefix.to_string(),
            number,
        };
        (id, false)
    };

    let is_closed = full.state == RemoteState::Closed;

    // Build comments: body as first comment, then actual comments
    let mut comments = Vec::new();
    comments.push(Comment {
        author: full.author.clone(),
        timestamp: Some(full.created_at.naive_utc()),
        body: full.body.clone(),
    });
    for rc in &full.comments {
        comments.push(Comment {
            author: rc.author.clone(),
            timestamp: Some(rc.created_at.naive_utc()),
            body: rc.body.clone(),
        });
    }

    let mut ticket = Ticket {
        id: id.clone(),
        title: full.title.clone(),
        status: existing.and_then(|t| t.status.clone()),
        frontmatter: existing.and_then(|t| t.frontmatter.clone()),
        preamble: existing.map(|t| t.preamble.clone()).unwrap_or_default(),
        comments,
        is_closed,
    };

    // Compute hash and update metadata
    let hash = compute_content_hash(&ticket, default_status);
    let metadata = SyncMetadata {
        provider: provider.name().to_string(),
        repo: sync_config.repo.clone(),
        remote_id: full.remote_id.clone(),
        content_hash: hash,
        remote_updated: full.updated_at,
        comment_count: ticket.comments.len(),
    };
    ticket.frontmatter = Some(metadata.merge_into_frontmatter(ticket.frontmatter.as_deref()));

    store.write_ticket(&ticket, default_status)?;

    // Handle open/closed state transitions
    if is_closed && !was_closed {
        let _ = store.move_to_done(&id);
    } else if !is_closed && was_closed {
        let _ = store.move_from_done(&id);
    }

    let action = if existing.is_some() { "Updated" } else { "Created" };
    println!("  {} {} <- remote #{} \"{}\"", action, id, full.remote_id, full.title);

    Ok(())
}

/// Push new local comments to a remote issue.
fn push_comments(
    store: &TicketStore,
    ticket: &Ticket,
    metadata: &SyncMetadata,
    provider: &dyn SyncProvider,
    sync_config: &SyncConfig,
    default_status: &str,
) -> Result<()> {
    let new_comments = &ticket.comments[metadata.comment_count..];
    for comment in new_comments {
        let body = format_comment_for_push(comment);
        provider.add_comment(&metadata.remote_id, &body)?;
    }

    // Re-fetch to get updated_at, then update metadata
    let updated = provider.fetch_issue(&metadata.remote_id)?;
    let mut ticket = ticket.clone();
    let hash = compute_content_hash(&ticket, default_status);
    let new_meta = SyncMetadata {
        provider: provider.name().to_string(),
        repo: sync_config.repo.clone(),
        remote_id: metadata.remote_id.clone(),
        content_hash: hash,
        remote_updated: updated.updated_at,
        comment_count: ticket.comments.len(),
    };
    ticket.frontmatter = Some(new_meta.merge_into_frontmatter(ticket.frontmatter.as_deref()));
    store.write_ticket(&ticket, default_status)?;

    println!(
        "  Pushed {} comment(s) for {} -> remote #{}",
        new_comments.len(),
        ticket.id,
        metadata.remote_id
    );
    Ok(())
}

/// Push a locally-created ticket as a new remote issue.
fn push_new_issue(
    store: &TicketStore,
    ticket: &Ticket,
    provider: &dyn SyncProvider,
    sync_config: &SyncConfig,
    default_status: &str,
) -> Result<()> {
    let body = if ticket.preamble.is_empty() {
        String::new()
    } else {
        ticket.preamble.clone()
    };

    let created = provider.create_issue(&ticket.title, &body)?;

    // Push each comment
    for comment in &ticket.comments {
        let text = format_comment_for_push(comment);
        provider.add_comment(&created.remote_id, &text)?;
    }

    // Re-fetch to get final updated_at
    let updated = provider.fetch_issue(&created.remote_id)?;

    let mut ticket = ticket.clone();
    let hash = compute_content_hash(&ticket, default_status);
    let metadata = SyncMetadata {
        provider: provider.name().to_string(),
        repo: sync_config.repo.clone(),
        remote_id: created.remote_id.clone(),
        content_hash: hash,
        remote_updated: updated.updated_at,
        comment_count: ticket.comments.len(),
    };
    ticket.frontmatter = Some(metadata.merge_into_frontmatter(ticket.frontmatter.as_deref()));
    store.write_ticket(&ticket, default_status)?;

    println!(
        "  Pushed {} -> remote #{} \"{}\"",
        ticket.id, created.remote_id, ticket.title
    );
    Ok(())
}

fn format_comment_for_push(comment: &Comment) -> String {
    let mut parts = Vec::new();
    if let Some(ts) = comment.timestamp {
        parts.push(format!("*{} ({})*", comment.author, ts.format("%Y-%m-%d %H:%M")));
    } else {
        parts.push(format!("*{}*", comment.author));
    }
    if !comment.body.is_empty() {
        parts.push(String::new());
        parts.push(comment.body.clone());
    }
    parts.join("\n")
}

/// Show sync status for a project (read-only, no remote fetch).
pub fn show_status(config: &Config, sync_config: &SyncConfig) -> Result<()> {
    let prefix = config.resolve_prefix(&sync_config.project).ok_or_else(|| {
        anyhow::anyhow!("Unknown project: {}", sync_config.project)
    })?;
    let store = TicketStore::new(&config.notes_dir);
    let default_status = &config.statuses.default;

    let all_tickets = store.scan_tickets(true)?;
    let project_tickets: Vec<Ticket> = all_tickets
        .into_iter()
        .filter(|t| t.id.prefix == prefix)
        .collect();

    println!(
        "Project: {} ({}) -> {} ({})",
        sync_config.project, prefix, sync_config.provider, sync_config.repo
    );

    let mut synced = 0;
    let mut dirty = 0;
    let mut unsynced = 0;

    for ticket in &project_tickets {
        if let Some(ref fm) = ticket.frontmatter {
            if let Some(meta) = SyncMetadata::from_frontmatter(fm) {
                synced += 1;
                let current_hash = compute_content_hash(ticket, default_status);
                if current_hash != meta.content_hash {
                    dirty += 1;
                    println!("  {} (remote #{}) — DIRTY (local changes)", ticket.id, meta.remote_id);
                }
                continue;
            }
        }
        unsynced += 1;
        println!("  {} — UNSYNCED (will be pushed on next sync)", ticket.id);
    }

    println!(
        "  Total: {} synced ({} dirty), {} unsynced",
        synced, dirty, unsynced
    );
    Ok(())
}

/// Initialize sync: add config entry and do initial pull.
pub fn init_sync(
    config: &Config,
    sync_config: &SyncConfig,
    provider: &dyn SyncProvider,
) -> Result<()> {
    let prefix = config.resolve_prefix(&sync_config.project).ok_or_else(|| {
        anyhow::anyhow!("Unknown project: {}", sync_config.project)
    })?;
    let store = TicketStore::new(&config.notes_dir);
    let default_status = &config.statuses.default;

    // Fetch all remote issues
    println!("Fetching issues from {} ...", sync_config.repo);
    let issues = provider.list_issues()?;
    println!("Found {} issues. Downloading...", issues.len());

    let mut count = 0;
    for (i, issue) in issues.iter().enumerate() {
        println!(
            "  [{}/{}] Downloading #{} \"{}\"...",
            i + 1,
            issues.len(),
            issue.remote_id,
            issue.title
        );
        pull_issue(
            &store,
            None,
            issue,
            &prefix,
            provider,
            sync_config,
            default_status,
        )?;
        count += 1;
    }

    println!("Downloaded {} issues.", count);
    Ok(())
}

/// Append a [[sync]] entry to the config file.
pub fn append_sync_to_config(config: &Config, sync_config: &SyncConfig) -> Result<()> {
    let content = std::fs::read_to_string(&config.config_path)
        .with_context(|| format!("Failed to read config: {}", config.config_path.display()))?;

    let mut doc: toml::Value = content
        .parse()
        .with_context(|| "Failed to parse config as TOML")?;

    let entry = {
        let mut table = toml::value::Table::new();
        table.insert(
            "project".to_string(),
            toml::Value::String(sync_config.project.clone()),
        );
        table.insert(
            "provider".to_string(),
            toml::Value::String(sync_config.provider.clone()),
        );
        table.insert(
            "repo".to_string(),
            toml::Value::String(sync_config.repo.clone()),
        );
        toml::Value::Table(table)
    };

    match doc.get_mut("sync") {
        Some(toml::Value::Array(arr)) => {
            arr.push(entry);
        }
        None => {
            doc.as_table_mut()
                .unwrap()
                .insert("sync".to_string(), toml::Value::Array(vec![entry]));
        }
        _ => bail!("'sync' key in config is not an array"),
    }

    let new_content = toml::to_string_pretty(&doc)?;
    std::fs::write(&config.config_path, new_content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StatusConfig;
    use crate::sync::{CreatedIssue, RemoteComment};
    use chrono::Utc;
    use std::cell::RefCell;
    use std::collections::HashMap;

    struct MockProvider {
        issues: Vec<RemoteIssue>,
        created: RefCell<Vec<(String, String)>>,
        comments_pushed: RefCell<Vec<(String, String)>>,
    }

    impl MockProvider {
        fn new(issues: Vec<RemoteIssue>) -> Self {
            MockProvider {
                issues,
                created: RefCell::new(Vec::new()),
                comments_pushed: RefCell::new(Vec::new()),
            }
        }
    }

    impl SyncProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        fn check_availability(&self) -> Result<()> {
            Ok(())
        }

        fn list_issues(&self) -> Result<Vec<RemoteIssue>> {
            Ok(self.issues.clone())
        }

        fn fetch_issue(&self, remote_id: &str) -> Result<RemoteIssue> {
            self.issues
                .iter()
                .find(|i| i.remote_id == remote_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Issue {} not found", remote_id))
        }

        fn create_issue(&self, title: &str, body: &str) -> Result<CreatedIssue> {
            self.created
                .borrow_mut()
                .push((title.to_string(), body.to_string()));
            Ok(CreatedIssue {
                remote_id: "999".to_string(),
                updated_at: Utc::now(),
            })
        }

        fn add_comment(&self, remote_id: &str, body: &str) -> Result<()> {
            self.comments_pushed
                .borrow_mut()
                .push((remote_id.to_string(), body.to_string()));
            Ok(())
        }

        fn close_issue(&self, _remote_id: &str) -> Result<()> {
            Ok(())
        }

        fn reopen_issue(&self, _remote_id: &str) -> Result<()> {
            Ok(())
        }
    }

    fn test_config(dir: &std::path::Path) -> Config {
        let mut projects = HashMap::new();
        projects.insert("TestProject".to_string(), "TP".to_string());
        Config {
            default_author: "Tester".to_string(),
            notes_dir: dir.to_path_buf(),
            statuses: StatusConfig {
                values: vec!["New".into(), "Done".into()],
                default: "New".to_string(),
            },
            projects,
            sync: vec![],
            config_path: dir.join(".tickdown.toml"),
        }
    }

    fn test_sync_config() -> SyncConfig {
        SyncConfig {
            project: "TestProject".to_string(),
            provider: "mock".to_string(),
            repo: "owner/repo".to_string(),
        }
    }

    fn make_remote_issue(id: &str, title: &str, body: &str) -> RemoteIssue {
        RemoteIssue {
            remote_id: id.to_string(),
            title: title.to_string(),
            state: RemoteState::Open,
            body: body.to_string(),
            author: "remote-user".to_string(),
            created_at: Utc::now(),
            comments: vec![],
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_pull_new_creates_ticket() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let sync_cfg = test_sync_config();
        let issues = vec![make_remote_issue("1", "First Issue", "Issue body text")];
        let provider = MockProvider::new(issues);

        let summary = sync_project(&config, &sync_cfg, &provider).unwrap();
        assert_eq!(summary.pulled, 1);

        // Verify ticket was created
        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "TP".into(),
            number: 1,
        };
        let ticket = store.read_ticket(&id).unwrap();
        assert_eq!(ticket.title, "First Issue");
        assert_eq!(ticket.comments.len(), 1); // body as first comment
        assert_eq!(ticket.comments[0].body, "Issue body text");
        assert!(ticket.frontmatter.is_some());
        let meta = SyncMetadata::from_frontmatter(ticket.frontmatter.as_ref().unwrap()).unwrap();
        assert_eq!(meta.remote_id, "1");
    }

    #[test]
    fn test_pull_with_comments() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let sync_cfg = test_sync_config();
        let mut issue = make_remote_issue("1", "With Comments", "Body");
        issue.comments = vec![
            RemoteComment {
                author: "commenter".to_string(),
                body: "Nice!".to_string(),
                created_at: Utc::now(),
            },
        ];
        let provider = MockProvider::new(vec![issue]);

        let summary = sync_project(&config, &sync_cfg, &provider).unwrap();
        assert_eq!(summary.pulled, 1);

        let store = TicketStore::new(dir.path());
        let id = TicketId {
            prefix: "TP".into(),
            number: 1,
        };
        let ticket = store.read_ticket(&id).unwrap();
        assert_eq!(ticket.comments.len(), 2); // body + 1 comment
        assert_eq!(ticket.comments[0].body, "Body");
        assert_eq!(ticket.comments[1].author, "commenter");
        assert_eq!(ticket.comments[1].body, "Nice!");
    }

    #[test]
    fn test_push_new_local_ticket() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let sync_cfg = test_sync_config();

        // Create a local ticket without sync metadata
        let store = TicketStore::new(dir.path());
        let ticket = Ticket {
            id: TicketId {
                prefix: "TP".to_string(),
                number: 1,
            },
            title: "Local ticket".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        store.write_ticket(&ticket, "New").unwrap();

        // Mock provider returns the created issue on fetch
        let mut provider = MockProvider::new(vec![]);
        // We need the provider to return the issue on fetch_issue("999") after creation
        provider.issues.push(RemoteIssue {
            remote_id: "999".to_string(),
            title: "Local ticket".to_string(),
            state: RemoteState::Open,
            body: String::new(),
            author: "Tester".to_string(),
            created_at: Utc::now(),
            comments: vec![],
            updated_at: Utc::now(),
        });

        let summary = sync_project(&config, &sync_cfg, &provider).unwrap();
        assert_eq!(summary.pushed_new, 1);
        assert_eq!(provider.created.borrow().len(), 1);

        // Ticket should now have sync metadata
        let ticket = store.read_ticket(&ticket.id).unwrap();
        assert!(ticket.frontmatter.is_some());
        let meta = SyncMetadata::from_frontmatter(ticket.frontmatter.as_ref().unwrap()).unwrap();
        assert_eq!(meta.remote_id, "999");
    }

    #[test]
    fn test_unchanged_ticket_no_action() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let sync_cfg = test_sync_config();

        let remote_updated = Utc::now();

        // Create a synced local ticket
        let store = TicketStore::new(dir.path());
        let mut ticket = Ticket {
            id: TicketId {
                prefix: "TP".to_string(),
                number: 1,
            },
            title: "Synced".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![Comment {
                author: "remote-user".to_string(),
                timestamp: Some(remote_updated.naive_utc()),
                body: "Body".to_string(),
            }],
            is_closed: false,
        };
        let hash = compute_content_hash(&ticket, "New");
        let meta = SyncMetadata {
            provider: "mock".to_string(),
            repo: "owner/repo".to_string(),
            remote_id: "1".to_string(),
            content_hash: hash,
            remote_updated,
            comment_count: 1,
        };
        ticket.frontmatter = Some(meta.to_frontmatter_string());
        store.write_ticket(&ticket, "New").unwrap();

        // Remote has same updated_at
        let remote = RemoteIssue {
            remote_id: "1".to_string(),
            title: "Synced".to_string(),
            state: RemoteState::Open,
            body: "Body".to_string(),
            author: "remote-user".to_string(),
            created_at: remote_updated,
            comments: vec![],
            updated_at: remote_updated,
        };
        let provider = MockProvider::new(vec![remote]);

        let summary = sync_project(&config, &sync_cfg, &provider).unwrap();
        assert_eq!(summary.unchanged, 1);
        assert_eq!(summary.pulled, 0);
        assert_eq!(summary.pushed_comments, 0);
    }

    #[test]
    fn test_format_comment_for_push() {
        use chrono::NaiveDate;
        let comment = Comment {
            author: "Lars".to_string(),
            timestamp: Some(
                NaiveDate::from_ymd_opt(2026, 3, 21)
                    .unwrap()
                    .and_hms_opt(14, 30, 0)
                    .unwrap(),
            ),
            body: "Did the thing.".to_string(),
        };
        let result = format_comment_for_push(&comment);
        assert!(result.contains("*Lars (2026-03-21 14:30)*"));
        assert!(result.contains("Did the thing."));
    }
}
