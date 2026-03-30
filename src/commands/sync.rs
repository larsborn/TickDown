use anyhow::{bail, Result};

use crate::config::{Config, SyncConfig};
use crate::sync::github::GitHubProvider;
use crate::sync::orchestrator;
use crate::sync::SyncProvider;

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
