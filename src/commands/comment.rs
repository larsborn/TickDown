use anyhow::Result;
use chrono::Local;

use crate::config::Config;
use crate::store::TicketStore;
use crate::ticket::{Comment, TicketId};

pub fn run(config: &Config, ticket_id: &str, text: &str) -> Result<()> {
    let id = TicketId::parse(ticket_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid ticket ID: {}", ticket_id))?;

    let store = TicketStore::new(&config.notes_dir);
    let mut ticket = store.read_ticket(&id)?;

    let now = Local::now().naive_local();
    ticket.comments.push(Comment {
        author: config.default_author.clone(),
        timestamp: Some(now),
        body: text.to_string(),
    });

    let path = store.write_ticket(&ticket, &config.statuses.default)?;
    println!("Comment added to {} ({})", ticket.id, path.display());
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
    fn test_comment_appends_to_ticket() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        // Create a ticket first
        std::fs::write(
            dir.path().join("TD-1 Test.md"),
            "# TD-1 Test\n* Status: New\n\n## Author (2026-01-01 10:00)\nInitial.\n",
        )
        .unwrap();

        run(&config, "TD-1", "My comment text").unwrap();

        let content = std::fs::read_to_string(dir.path().join("TD-1 Test.md")).unwrap();
        assert!(content.contains("My comment text"));
        assert!(content.contains("## Tester ("));
        // Should still have old comment
        assert!(content.contains("Initial."));
    }

    #[test]
    fn test_comment_normalizes_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        // Create a ticket with non-canonical format (missing status)
        std::fs::write(
            dir.path().join("TD-1 Test.md"),
            "# TD-1 Test\n\n## Author (2026-01-01)\nOld comment.\n",
        )
        .unwrap();

        run(&config, "TD-1", "New comment").unwrap();

        let content = std::fs::read_to_string(dir.path().join("TD-1 Test.md")).unwrap();
        // Status should be added
        assert!(content.contains("* Status: New"));
        // Old comment date should get time appended
        assert!(content.contains("(2026-01-01 00:00)"));
    }

    #[test]
    fn test_comment_invalid_id() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "invalid", "text");
        assert!(result.is_err());
    }

    #[test]
    fn test_comment_ticket_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "TD-99", "text");
        assert!(result.is_err());
    }
}
