use std::io::{self, BufRead, Write};

use anyhow::{bail, Result};

use crate::config::{Config, SyncConfig};
use crate::store::TicketStore;
use crate::sync::github::GitHubProvider;
use crate::sync::metadata::SyncMetadata;
use crate::sync::orchestrator;
use crate::sync::SyncProvider;
use crate::ticket::TicketId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveStrategy {
    /// Discard local, take remote.
    Pull,
    /// Keep local content as the new baseline; pushes new local
    /// comments first, then refreshes sync metadata against the
    /// current remote without pulling remote-side changes.
    KeepLocal,
}

pub fn init(config: &Config, provider_name: &str, repo: &str, project: &str) -> Result<()> {
    if provider_name != "github" {
        bail!("Unknown sync provider: {}. Supported: github", provider_name);
    }

    config.resolve_prefix(project).ok_or_else(|| {
        let known: Vec<_> = config
            .projects
            .iter()
            .map(|(name, pfx)| format!("{} ({})", name, pfx))
            .collect();
        anyhow::anyhow!(
            "Unknown project: {}. Known: {}",
            project,
            known.join(", ")
        )
    })?;

    // Check for duplicate sync config
    for sc in &config.sync {
        if sc.project == project && sc.provider == provider_name && sc.repo == repo {
            bail!(
                "Sync already configured for project {} with {} ({})",
                project,
                provider_name,
                repo
            );
        }
    }

    let provider = GitHubProvider::new(repo);
    provider.check_availability()?;

    let sync_config = SyncConfig {
        project: project.to_string(),
        provider: provider_name.to_string(),
        repo: repo.to_string(),
    };

    // Append to config file
    orchestrator::append_sync_to_config(config, &sync_config)?;
    println!(
        "Configured sync: {} ({}) <-> {} ({})",
        project,
        config.resolve_prefix(project).unwrap(),
        provider_name,
        repo
    );

    // Do initial pull
    orchestrator::init_sync(config, &sync_config, &provider)?;

    Ok(())
}

pub fn run(config: &Config, project: Option<&str>) -> Result<()> {
    let sync_configs = resolve_sync_configs(config, project)?;

    for sync_config in &sync_configs {
        println!("Syncing {} ({})...", sync_config.project, sync_config.repo);

        let provider = create_provider(sync_config)?;
        provider.check_availability()?;

        let summary = orchestrator::sync_project(config, sync_config, provider.as_ref())?;
        summary.print();
    }

    Ok(())
}

pub fn status(config: &Config, project: Option<&str>) -> Result<()> {
    let sync_configs = resolve_sync_configs(config, project)?;

    if sync_configs.is_empty() {
        println!("No sync configurations found.");
        return Ok(());
    }

    for sync_config in &sync_configs {
        orchestrator::show_status(config, sync_config)?;
    }

    Ok(())
}

fn resolve_sync_configs<'a>(
    config: &'a Config,
    project: Option<&str>,
) -> Result<Vec<&'a SyncConfig>> {
    if let Some(project) = project {
        let configs: Vec<_> = config
            .sync
            .iter()
            .filter(|sc| sc.project == project)
            .collect();
        if configs.is_empty() {
            bail!("No sync configuration found for project: {}", project);
        }
        Ok(configs)
    } else {
        Ok(config.sync.iter().collect())
    }
}

fn create_provider(sync_config: &SyncConfig) -> Result<Box<dyn crate::sync::SyncProvider>> {
    match sync_config.provider.as_str() {
        "github" => Ok(Box::new(GitHubProvider::new(&sync_config.repo))),
        other => bail!("Unknown sync provider: {}", other),
    }
}

/// Force-resolve a sync conflict on a single ticket.
///
/// Validation order (so tests can exercise each path without a real
/// provider): parse id → load ticket → require sync metadata → find
/// matching `[[sync]]` config → confirm → create provider → execute.
pub fn resolve(
    config: &Config,
    ticket_id: &str,
    strategy: ResolveStrategy,
    yes: bool,
) -> Result<()> {
    let id = TicketId::parse(ticket_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid ticket ID: {}", ticket_id))?;

    let store = TicketStore::new(&config.notes_dir);
    let ticket = store.read_ticket(&id)?;

    let metadata = ticket
        .frontmatter
        .as_deref()
        .and_then(SyncMetadata::from_frontmatter)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Ticket {} is not synced (no sync_* frontmatter). \
                 Run `td sync init` to set up sync first.",
                id
            )
        })?;

    let sync_config = config
        .sync
        .iter()
        .find(|sc| sc.provider == metadata.provider && sc.repo == metadata.repo)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No [[sync]] config matches ticket {}: provider={}, repo={}. \
                 Add an entry with `td sync init` or edit .tickdown.toml.",
                id,
                metadata.provider,
                metadata.repo,
            )
        })?;

    if !yes && !confirm_resolve(&id, &metadata, sync_config, strategy)? {
        println!("Aborted.");
        return Ok(());
    }

    let provider = create_provider(sync_config)?;
    provider.check_availability()?;

    match strategy {
        ResolveStrategy::Pull => {
            orchestrator::pull_issue(
                &store,
                Some(&ticket),
                &metadata.remote_id,
                &id.prefix,
                provider.as_ref(),
                sync_config,
                &config.statuses.default,
            )?;
        }
        ResolveStrategy::KeepLocal => {
            let pushed = orchestrator::rebaseline_local(
                &store,
                &ticket,
                &metadata,
                provider.as_ref(),
                sync_config,
                &config.statuses.default,
            )?;
            if pushed > 0 {
                println!(
                    "  Re-baselined {} (remote #{}); pushed {} new comment(s).",
                    id, metadata.remote_id, pushed
                );
            } else {
                println!(
                    "  Re-baselined {} (remote #{}); local content kept, \
                     remote-side changes since last sync are not merged.",
                    id, metadata.remote_id
                );
            }
        }
    }

    Ok(())
}

fn confirm_resolve(
    id: &TicketId,
    metadata: &SyncMetadata,
    sync_config: &SyncConfig,
    strategy: ResolveStrategy,
) -> Result<bool> {
    let prompt = match strategy {
        ResolveStrategy::Pull => format!(
            "This will OVERWRITE local {} with remote {}#{}.\n\
             Any local changes (comments, preamble, status) will be lost.\n\
             Continue? [y/N] ",
            id, sync_config.repo, metadata.remote_id,
        ),
        ResolveStrategy::KeepLocal => format!(
            "This will RE-BASELINE local {} (remote {}#{}).\n\
             New local comments will be pushed first, then sync metadata is \
             reset to the current remote pointer. Remote-side title/body/status \
             changes since the last sync will NOT be merged.\n\
             Continue? [y/N] ",
            id, sync_config.repo, metadata.remote_id,
        ),
    };
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "YES"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StatusConfig;
    use std::collections::HashMap;

    fn make_config(sync: Vec<SyncConfig>) -> Config {
        let mut projects = HashMap::new();
        projects.insert("TickDown".to_string(), "LAW".to_string());
        projects.insert("Other".to_string(), "OT".to_string());
        Config {
            default_author: "Tester".to_string(),
            notes_dir: std::path::PathBuf::from("."),
            statuses: StatusConfig {
                values: vec!["New".to_string()],
                default: "New".to_string(),
            },
            projects,
            sync,
            config_path: std::path::PathBuf::from(".tickdown.toml"),
        }
    }

    // --- init validation paths ---

    #[test]
    fn test_init_rejects_unknown_provider() {
        let config = make_config(vec![]);
        let result = init(&config, "jira", "owner/repo", "TickDown");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Unknown sync provider"));
    }

    #[test]
    fn test_init_rejects_unknown_project() {
        let config = make_config(vec![]);
        let result = init(&config, "github", "owner/repo", "DoesNotExist");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Unknown project"));
    }

    #[test]
    fn test_init_rejects_duplicate_sync() {
        let config = make_config(vec![SyncConfig {
            project: "TickDown".to_string(),
            provider: "github".to_string(),
            repo: "owner/repo".to_string(),
        }]);
        let result = init(&config, "github", "owner/repo", "TickDown");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("already configured"));
    }

    // --- resolve_sync_configs ---

    #[test]
    fn test_resolve_sync_configs_all() {
        let config = make_config(vec![
            SyncConfig {
                project: "TickDown".to_string(),
                provider: "github".to_string(),
                repo: "a/b".to_string(),
            },
            SyncConfig {
                project: "Other".to_string(),
                provider: "github".to_string(),
                repo: "c/d".to_string(),
            },
        ]);
        let configs = resolve_sync_configs(&config, None).unwrap();
        assert_eq!(configs.len(), 2);
    }

    #[test]
    fn test_resolve_sync_configs_filtered() {
        let config = make_config(vec![
            SyncConfig {
                project: "TickDown".to_string(),
                provider: "github".to_string(),
                repo: "a/b".to_string(),
            },
            SyncConfig {
                project: "Other".to_string(),
                provider: "github".to_string(),
                repo: "c/d".to_string(),
            },
        ]);
        let configs = resolve_sync_configs(&config, Some("TickDown")).unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].project, "TickDown");
    }

    #[test]
    fn test_resolve_sync_configs_not_found() {
        let config = make_config(vec![SyncConfig {
            project: "TickDown".to_string(),
            provider: "github".to_string(),
            repo: "a/b".to_string(),
        }]);
        let result = resolve_sync_configs(&config, Some("Other"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No sync configuration"));
    }

    #[test]
    fn test_resolve_sync_configs_all_empty() {
        let config = make_config(vec![]);
        let configs = resolve_sync_configs(&config, None).unwrap();
        assert!(configs.is_empty());
    }

    // --- status for empty sync list ---

    #[test]
    fn test_status_with_no_sync_configs() {
        let config = make_config(vec![]);
        // Should not error, just print "No sync configurations found."
        status(&config, None).unwrap();
    }

    // --- create_provider ---

    #[test]
    fn test_create_provider_github() {
        let sc = SyncConfig {
            project: "TickDown".to_string(),
            provider: "github".to_string(),
            repo: "owner/repo".to_string(),
        };
        let provider = create_provider(&sc).unwrap();
        assert_eq!(provider.name(), "github");
    }

    #[test]
    fn test_create_provider_unknown() {
        let sc = SyncConfig {
            project: "TickDown".to_string(),
            provider: "jira".to_string(),
            repo: "owner/repo".to_string(),
        };
        match create_provider(&sc) {
            Ok(_) => panic!("expected error"),
            Err(e) => assert!(e.to_string().contains("Unknown sync provider")),
        }
    }

    // --- resolve validation paths ---

    fn make_config_with_dir(dir: &std::path::Path, sync: Vec<SyncConfig>) -> Config {
        let mut projects = HashMap::new();
        projects.insert("TickDown".to_string(), "LAW".to_string());
        Config {
            default_author: "Tester".to_string(),
            notes_dir: dir.to_path_buf(),
            statuses: StatusConfig {
                values: vec!["New".to_string()],
                default: "New".to_string(),
            },
            projects,
            sync,
            config_path: dir.join(".tickdown.toml"),
        }
    }

    #[test]
    fn test_resolve_invalid_ticket_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let config = make_config_with_dir(dir.path(), vec![]);
        let result = resolve(&config, "not-a-valid-id", ResolveStrategy::Pull, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.to_lowercase().contains("invalid"));
    }

    #[test]
    fn test_resolve_unknown_ticket_errors() {
        let dir = tempfile::tempdir().unwrap();
        let config = make_config_with_dir(dir.path(), vec![]);
        let result = resolve(&config, "LAW-999", ResolveStrategy::Pull, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_resolve_unsynced_ticket_errors() {
        let dir = tempfile::tempdir().unwrap();
        let config = make_config_with_dir(dir.path(), vec![]);
        // Write a ticket without any sync_* frontmatter.
        std::fs::write(
            dir.path().join("LAW-1 Plain.md"),
            "# LAW-1 Plain\n* Status: New\n",
        )
        .unwrap();
        let result = resolve(&config, "LAW-1", ResolveStrategy::Pull, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not synced"));
    }

    #[test]
    fn test_resolve_no_matching_sync_config_errors() {
        let dir = tempfile::tempdir().unwrap();
        // Config has a [[sync]] entry but for a different repo than the
        // ticket's frontmatter points to.
        let config = make_config_with_dir(
            dir.path(),
            vec![SyncConfig {
                project: "TickDown".to_string(),
                provider: "github".to_string(),
                repo: "other/elsewhere".to_string(),
            }],
        );
        // Ticket frontmatter says repo "missing/repo".
        std::fs::write(
            dir.path().join("LAW-1 Foreign.md"),
            "---\nsync_provider: github\nsync_repo: missing/repo\nsync_issue: 7\nsync_hash: abc\nsync_remote_updated: 2026-01-01T00:00:00+00:00\nsync_comment_count: 0\n---\n# LAW-1 Foreign\n* Status: New\n",
        )
        .unwrap();
        let result = resolve(&config, "LAW-1", ResolveStrategy::KeepLocal, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("missing/repo") || msg.to_lowercase().contains("no [[sync]]"));
    }
}
