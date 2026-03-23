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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, StatusConfig};
    use std::collections::HashMap;

    fn setup_test(dir: &std::path::Path) -> Config {
        let config_path = dir.join(".tickdown.toml");
        std::fs::write(
            &config_path,
            r#"
default_author = "Tester"
notes_dir = "."

[statuses]
values = ["New", "Done"]
default = "New"

[projects]
TickDown = "TD"
Other = "OTH"
"#,
        )
        .unwrap();

        let mut projects = HashMap::new();
        projects.insert("TickDown".to_string(), "TD".to_string());
        projects.insert("Other".to_string(), "OTH".to_string());
        Config {
            default_author: "Tester".to_string(),
            notes_dir: dir.to_path_buf(),
            statuses: StatusConfig {
                values: vec!["New".into(), "Done".into()],
                default: "New".to_string(),
            },
            projects,
            config_path,
        }
    }

    #[test]
    fn test_rename_project_changes_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());

        std::fs::write(
            dir.path().join("TD-1 First.md"),
            "# TD-1 First\n* Status: New\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("TD-2 Second.md"),
            "# TD-2 Second\n* Status: New\n",
        )
        .unwrap();

        run(&config, "TickDown", None, Some("NEW")).unwrap();

        assert!(!dir.path().join("TD-1 First.md").exists());
        assert!(!dir.path().join("TD-2 Second.md").exists());
        assert!(dir.path().join("NEW-1 First.md").exists());
        assert!(dir.path().join("NEW-2 Second.md").exists());

        // Content should have new prefix
        let content = std::fs::read_to_string(dir.path().join("NEW-1 First.md")).unwrap();
        assert!(content.contains("NEW-1"));
        assert!(!content.contains("TD-1"));

        // Config should be updated
        let cfg = std::fs::read_to_string(dir.path().join(".tickdown.toml")).unwrap();
        assert!(cfg.contains("NEW"));
    }

    #[test]
    fn test_rename_project_changes_name() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());

        run(&config, "TickDown", Some("MyProject"), None).unwrap();

        let cfg = std::fs::read_to_string(dir.path().join(".tickdown.toml")).unwrap();
        assert!(cfg.contains("MyProject"));
        assert!(!cfg.contains("TickDown"));
        // Prefix should be unchanged
        assert!(cfg.contains("TD"));
    }

    #[test]
    fn test_rename_project_includes_done_dir() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());

        std::fs::write(
            dir.path().join("TD-1 Open.md"),
            "# TD-1 Open\n* Status: New\n",
        )
        .unwrap();
        let done = dir.path().join("done");
        std::fs::create_dir(&done).unwrap();
        std::fs::write(done.join("TD-2 Closed.md"), "# TD-2 Closed\n* Status: Done\n").unwrap();

        run(&config, "TickDown", None, Some("NEW")).unwrap();

        assert!(dir.path().join("NEW-1 Open.md").exists());
        assert!(done.join("NEW-2 Closed.md").exists());
    }

    #[test]
    fn test_rename_project_no_args() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());
        let result = run(&config, "TickDown", None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--name or --prefix"));
    }

    #[test]
    fn test_rename_project_prefix_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());
        // OTH prefix already used by "Other"
        let result = run(&config, "TickDown", None, Some("OTH"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already used"));
    }

    #[test]
    fn test_rename_project_name_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());
        let result = run(&config, "TickDown", Some("Other"), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_rename_project_both_name_and_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());

        std::fs::write(
            dir.path().join("TD-1 Test.md"),
            "# TD-1 Test\n* Status: New\n",
        )
        .unwrap();

        run(&config, "TickDown", Some("NewName"), Some("NEW")).unwrap();

        // File should be renamed
        assert!(dir.path().join("NEW-1 Test.md").exists());

        // Config should have new name and new prefix
        let cfg = std::fs::read_to_string(dir.path().join(".tickdown.toml")).unwrap();
        assert!(cfg.contains("NewName"));
        assert!(cfg.contains("NEW"));
        assert!(!cfg.contains("TickDown"));
    }

    #[test]
    fn test_rename_project_no_ticket_files() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());

        // No TD-* files exist
        run(&config, "TickDown", None, Some("NEW")).unwrap();

        let cfg = std::fs::read_to_string(dir.path().join(".tickdown.toml")).unwrap();
        assert!(cfg.contains("NEW"));
    }

    #[test]
    fn test_rename_project_unknown_project() {
        let dir = tempfile::tempdir().unwrap();
        let config = setup_test(dir.path());
        let result = run(&config, "Nonexistent", Some("Foo"), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown project or prefix"));
    }
}
