use anyhow::Result;

use crate::config::Config;
use crate::store::TicketStore;
use crate::ticket::TicketId;

pub fn run(config: &Config, ticket_id: &str) -> Result<()> {
    let id = TicketId::parse(ticket_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid ticket ID: {}", ticket_id))?;

    let store = TicketStore::new(&config.notes_dir);
    let dest = store.move_to_done(&id)?;
    println!("Closed {} -> {}", id, dest.display());
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
            sync: vec![],
            config_path: dir.join(".tickdown.toml"),
        }
    }

    #[test]
    fn test_close_moves_to_done() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        let ticket_file = dir.path().join("TD-1 Test.md");
        std::fs::write(&ticket_file, "# TD-1 Test\n* Status: New\n").unwrap();

        run(&config, "TD-1").unwrap();

        assert!(!ticket_file.exists());
        assert!(dir.path().join("done").join("TD-1 Test.md").exists());
    }

    #[test]
    fn test_close_invalid_id() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "bad");
        assert!(result.is_err());
    }

    #[test]
    fn test_close_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "TD-99");
        assert!(result.is_err());
    }
}
