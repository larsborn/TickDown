use anyhow::{bail, Context, Result};

use crate::config::Config;
use crate::parser::parse_filename;
use crate::store::TicketStore;
use crate::ticket::TicketId;

pub fn run(
    config: &Config,
    current: &str,
    new_name: Option<&str>,
    new_prefix: Option<&str>,
) -> Result<()> {
    if new_name.is_none() && new_prefix.is_none() {
        bail!("Provide at least one of --name or --prefix.");
    }

    // Resolve current identifier to project name + prefix
    let old_prefix = config.resolve_prefix(current).ok_or_else(|| {
        let known: Vec<_> = config
            .projects
            .iter()
            .map(|(name, pfx)| format!("{} ({})", name, pfx))
            .collect();
        anyhow::anyhow!(
            "Unknown project or prefix: {}. Known: {}",
            current,
            known.join(", ")
        )
    })?;
    let old_name = config
        .project_name_for_prefix(&old_prefix)
        .ok_or_else(|| anyhow::anyhow!("No project name found for prefix {}", old_prefix))?
        .to_string();

    let target_prefix = new_prefix.unwrap_or(&old_prefix);
    let target_name = new_name.unwrap_or(&old_name);

    // Validate new prefix doesn't collide with an existing different project
    if target_prefix != old_prefix {
        if let Some(existing) = config.project_name_for_prefix(target_prefix) {
            bail!(
                "Prefix {} is already used by project {}",
                target_prefix,
                existing
            );
        }
    }

    // Validate new name doesn't collide with an existing different project
    if target_name != old_name {
        if config.prefix_for_project(target_name).is_some() {
            bail!("Project name {} already exists", target_name);
        }
    }

    // Rename files if prefix changed
    if target_prefix != old_prefix {
        rename_ticket_files(config, &old_prefix, target_prefix)?;
    }

    // Update config file
    update_config_file(config, &old_name, target_name, target_prefix)?;

    if target_prefix != old_prefix && target_name != old_name {
        println!(
            "Renamed project {} ({}) -> {} ({})",
            old_name, old_prefix, target_name, target_prefix
        );
    } else if target_prefix != old_prefix {
        println!(
            "Changed prefix for {} : {} -> {}",
            target_name, old_prefix, target_prefix
        );
    } else {
        println!(
            "Renamed project {} -> {} (prefix {} unchanged)",
            old_name, target_name, old_prefix
        );
    }

    Ok(())
}

fn rename_ticket_files(config: &Config, old_prefix: &str, new_prefix: &str) -> Result<()> {
    let store = TicketStore::new(&config.notes_dir);

    // Check for number conflicts first
    let mut to_rename = Vec::new();
    for dir in [&store.root, &store.done_dir] {
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().map(|e| e != "md").unwrap_or(true) {
                continue;
            }
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            if let Some((id, _)) = parse_filename(&stem) {
                if id.prefix == old_prefix {
                    // Check no file with new prefix + same number exists
                    let new_id = TicketId {
                        prefix: new_prefix.to_string(),
                        number: id.number,
                    };
                    if store.find_ticket_file(&new_id).is_ok() {
                        bail!(
                            "Conflict: {} already exists, cannot rename {} -> {}",
                            new_id,
                            id,
                            new_id
                        );
                    }
                    to_rename.push((path, id));
                }
            }
        }
    }

    // Rename all files (simple prefix swap, no normalization)
    for (old_path, old_id) in &to_rename {
        let new_id = TicketId {
            prefix: new_prefix.to_string(),
            number: old_id.number,
        };

        // Replace prefix in file content (heading lines)
        let content = std::fs::read_to_string(old_path)
            .with_context(|| format!("Failed to read {}", old_path.display()))?;
        let old_id_str = old_id.to_string();
        let new_id_str = new_id.to_string();
        let new_content = content.replace(&old_id_str, &new_id_str);
        std::fs::write(old_path, &new_content)?;

        // Rename the file (swap prefix in filename, keep everything else)
        let old_name = old_path.file_name().unwrap().to_string_lossy();
        let new_name = old_name.replacen(&old_id_str, &new_id_str, 1);
        let new_path = old_path.with_file_name(&*new_name);

        if *old_path != new_path {
            std::fs::rename(old_path, &new_path)?;
        }

        println!("  {} -> {}", old_id, new_id);
    }

    if to_rename.is_empty() {
        println!("  (no ticket files to rename)");
    }

    Ok(())
}

fn update_config_file(
    config: &Config,
    old_name: &str,
    new_name: &str,
    new_prefix: &str,
) -> Result<()> {
    let content = std::fs::read_to_string(&config.config_path)
        .with_context(|| format!("Failed to read config: {}", config.config_path.display()))?;

    let mut doc: toml::Value = content
        .parse()
        .with_context(|| "Failed to parse config as TOML")?;

    if let Some(projects) = doc.get_mut("projects").and_then(|v| v.as_table_mut()) {
        // Remove old entry
        projects.remove(old_name);
        // Insert new entry
        projects.insert(
            new_name.to_string(),
            toml::Value::String(new_prefix.to_string()),
        );
    }

    let new_content = toml::to_string_pretty(&doc)?;
    std::fs::write(&config.config_path, new_content)?;

    Ok(())
}
