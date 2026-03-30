use anyhow::Result;
use chrono::Local;

use crate::config::Config;
use crate::store::TicketStore;
use crate::ticket::{Comment, Ticket, TicketId};

pub fn run(config: &Config, project: &str, title: &str) -> Result<()> {
    let prefix = config.resolve_prefix(project).ok_or_else(|| {
        let known: Vec<_> = config
            .projects
            .iter()
            .map(|(name, pfx)| format!("{} ({})", name, pfx))
            .collect();
        anyhow::anyhow!("Unknown project or prefix: {}. Known: {}", project, known.join(", "))
    })?;

    let store = TicketStore::new(&config.notes_dir);
    let number = store.next_number(&prefix)?;
    let id = TicketId {
        prefix,
        number,
    };

    let now = Local::now().naive_local();
    let ticket = Ticket {
        id: id.clone(),
        title: title.to_string(),
        status: Some(config.statuses.default.clone()),
        frontmatter: None,
        preamble: String::new(),
        comments: vec![Comment {
            author: config.default_author.clone(),
            timestamp: Some(now),
            body: String::new(),
        }],
        is_closed: false,
    };

    let path = store.write_ticket(&ticket, &config.statuses.default)?;
    println!("Created {} at {}", ticket.id, path.display());
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
    fn test_create_by_project_name() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        run(&config, "TickDown", "My first ticket").unwrap();

        let expected = dir.path().join("TD-1 My first ticket.md");
        assert!(expected.exists());
        let content = std::fs::read_to_string(&expected).unwrap();
        assert!(content.contains("# TD-1 My first ticket"));
        assert!(content.contains("* Status: New"));
    }

    #[test]
    fn test_create_by_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        run(&config, "TD", "Another ticket").unwrap();

        let expected = dir.path().join("TD-1 Another ticket.md");
        assert!(expected.exists());
    }

    #[test]
    fn test_create_auto_increments() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        run(&config, "TD", "First").unwrap();
        run(&config, "TD", "Second").unwrap();

        assert!(dir.path().join("TD-1 First.md").exists());
        assert!(dir.path().join("TD-2 Second.md").exists());
    }

    #[test]
    fn test_create_unknown_project() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "Unknown", "Title");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown project or prefix"));
    }
}
