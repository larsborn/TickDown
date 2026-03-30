use anyhow::{bail, Result};

use crate::config::Config;
use crate::store::TicketStore;
use crate::ticket::TicketId;

pub fn run(
    config: &Config,
    ticket_id: &str,
    new_project: Option<&str>,
    new_prefix: Option<&str>,
) -> Result<()> {
    if new_project.is_none() && new_prefix.is_none() {
        bail!("Provide --project or --prefix.");
    }
    if new_project.is_some() && new_prefix.is_some() {
        bail!("Provide either --project or --prefix, not both.");
    }

    let id = TicketId::parse(ticket_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid ticket ID: {}", ticket_id))?;

    let input = new_project.or(new_prefix).unwrap();
    let prefix = config.resolve_prefix(input).ok_or_else(|| {
        let known: Vec<_> = config
            .projects
            .iter()
            .map(|(name, pfx)| format!("{} ({})", name, pfx))
            .collect();
        anyhow::anyhow!(
            "Unknown project or prefix: {}. Known: {}",
            input,
            known.join(", ")
        )
    })?;

    let store = TicketStore::new(&config.notes_dir);
    let old_path = store.find_ticket_file(&id)?;
    let mut ticket = store.read_ticket(&id)?;

    if prefix == ticket.id.prefix {
        println!("{} is already in prefix {}", ticket.id, prefix);
        return Ok(());
    }

    let number = store.next_number(&prefix)?;
    let old_id = ticket.id.clone();
    ticket.id = TicketId { prefix, number };

    let new_path = store.write_ticket(&ticket, &config.statuses.default)?;

    if old_path != new_path && old_path.exists() {
        std::fs::remove_file(&old_path)?;
    }

    println!("Moved {} -> {} ({})", old_id, ticket.id, new_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, StatusConfig};
    use std::collections::HashMap;

    fn test_config(dir: &std::path::Path) -> Config {
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
            sync: vec![],
            config_path: dir.join(".tickdown.toml"),
        }
    }

    #[test]
    fn test_modify_moves_to_new_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(
            dir.path().join("TD-1 Test.md"),
            "# TD-1 Test\n* Status: New\n",
        )
        .unwrap();

        run(&config, "TD-1", Some("Other"), None).unwrap();

        assert!(!dir.path().join("TD-1 Test.md").exists());
        assert!(dir.path().join("OTH-1 Test.md").exists());
        let content = std::fs::read_to_string(dir.path().join("OTH-1 Test.md")).unwrap();
        assert!(content.contains("# OTH-1 Test"));
    }

    #[test]
    fn test_modify_by_prefix_directly() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(
            dir.path().join("TD-1 Test.md"),
            "# TD-1 Test\n* Status: New\n",
        )
        .unwrap();

        run(&config, "TD-1", None, Some("OTH")).unwrap();

        assert!(dir.path().join("OTH-1 Test.md").exists());
    }

    #[test]
    fn test_modify_same_prefix_noop() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(
            dir.path().join("TD-1 Test.md"),
            "# TD-1 Test\n* Status: New\n",
        )
        .unwrap();

        run(&config, "TD-1", Some("TickDown"), None).unwrap();

        // File should still be there unchanged
        assert!(dir.path().join("TD-1 Test.md").exists());
    }

    #[test]
    fn test_modify_no_args() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "TD-1", None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--project or --prefix"));
    }

    #[test]
    fn test_modify_both_args() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "TD-1", Some("Other"), Some("OTH"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not both"));
    }

    #[test]
    fn test_modify_unknown_project() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(
            dir.path().join("TD-1 Test.md"),
            "# TD-1 Test\n* Status: New\n",
        )
        .unwrap();

        let result = run(&config, "TD-1", Some("Nonexistent"), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown project or prefix"));
    }

    #[test]
    fn test_modify_invalid_id() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "bad", Some("Other"), None);
        assert!(result.is_err());
    }
}
