use anyhow::Result;

use crate::config::Config;
use crate::store::TicketStore;
use crate::ticket::TicketId;

pub fn run(config: &Config, ticket_id: &str, new_title: &str) -> Result<()> {
    let id = TicketId::parse(ticket_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid ticket ID: {}", ticket_id))?;

    let store = TicketStore::new(&config.notes_dir);
    let old_path = store.find_ticket_file(&id)?;
    let mut ticket = store.read_ticket(&id)?;

    ticket.title = new_title.to_string();

    let new_path = store.write_ticket(&ticket, &config.statuses.default)?;

    if old_path != new_path && old_path.exists() {
        std::fs::remove_file(&old_path)?;
    }

    println!("Renamed {} -> {} ({})", id, ticket.title, new_path.display());
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
        Config {
            default_author: "Tester".to_string(),
            notes_dir: dir.to_path_buf(),
            statuses: StatusConfig {
                values: vec!["New".into(), "Done".into()],
                default: "New".to_string(),
            },
            projects,
            config_path: dir.join(".tickdown.toml"),
        }
    }

    #[test]
    fn test_rename_changes_title() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        let old_file = dir.path().join("TD-1 Old Title.md");
        std::fs::write(&old_file, "# TD-1 Old Title\n* Status: New\n").unwrap();

        run(&config, "TD-1", "New Title").unwrap();

        assert!(!old_file.exists());
        let new_file = dir.path().join("TD-1 New Title.md");
        assert!(new_file.exists());
        let content = std::fs::read_to_string(&new_file).unwrap();
        assert!(content.contains("# TD-1 New Title"));
    }

    #[test]
    fn test_rename_invalid_id() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "nope", "Title");
        assert!(result.is_err());
    }

    #[test]
    fn test_rename_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "TD-99", "Title");
        assert!(result.is_err());
    }

    #[test]
    fn test_rename_preserves_comments() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(
            dir.path().join("TD-1 Old.md"),
            "# TD-1 Old\n* Status: In Progress\n\n## Author (2026-01-01 10:00)\nKeep me.\n",
        )
        .unwrap();

        run(&config, "TD-1", "New").unwrap();

        let content = std::fs::read_to_string(dir.path().join("TD-1 New.md")).unwrap();
        assert!(content.contains("# TD-1 New"));
        assert!(content.contains("Keep me."));
        assert!(content.contains("In Progress"));
    }
}
