use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::store::TicketStore;

pub fn run(
    config: &Config,
    prefix: Option<&str>,
    project: Option<&str>,
    all: bool,
) -> Result<()> {
    let store = TicketStore::new(&config.notes_dir);
    let tickets = store.scan_tickets(all)?;

    let filter_prefix = if let Some(proj) = project {
        Some(
            config
                .prefix_for_project(proj)
                .ok_or_else(|| anyhow::anyhow!("Unknown project: {}", proj))?,
        )
    } else {
        prefix
    };

    let filtered: Vec<_> = tickets
        .iter()
        .filter(|t| match filter_prefix {
            Some(p) => t.id.prefix == p,
            None => true,
        })
        .collect();

    if filtered.is_empty() {
        println!("No tickets found.");
        return Ok(());
    }

    for ticket in &filtered {
        let id_str = format!("{}", ticket.id);
        let status = ticket.status.as_deref().unwrap_or("-");
        let title = if ticket.title.is_empty() {
            "(no title)"
        } else {
            &ticket.title
        };

        let status_colored = match status {
            "Done" => status.green(),
            "In Progress" => status.yellow(),
            "Waiting For" => status.cyan(),
            _ => status.white(),
        };

        if ticket.is_closed {
            println!(
                "{:>12}  {:>14}  {} {}",
                id_str.dimmed(),
                status_colored,
                title.dimmed(),
                "(done)".dimmed()
            );
        } else {
            println!("{:>12}  {:>14}  {}", id_str.bold(), status_colored, title);
        }
    }

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
    fn test_list_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        run(&config, None, None, false).unwrap();
    }

    #[test]
    fn test_list_with_tickets() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(
            dir.path().join("TD-1 First.md"),
            "# TD-1 First\n* Status: New\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("TD-2 Second.md"),
            "# TD-2 Second\n* Status: In Progress\n",
        )
        .unwrap();

        run(&config, None, None, false).unwrap();
    }

    #[test]
    fn test_list_filter_by_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(dir.path().join("TD-1 A.md"), "# TD-1 A\n* Status: New\n").unwrap();
        std::fs::write(
            dir.path().join("OTH-1 B.md"),
            "# OTH-1 B\n* Status: New\n",
        )
        .unwrap();

        // Should not error
        run(&config, Some("TD"), None, false).unwrap();
    }

    #[test]
    fn test_list_filter_by_project() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(dir.path().join("TD-1 A.md"), "# TD-1 A\n* Status: New\n").unwrap();

        run(&config, None, Some("TickDown"), false).unwrap();
    }

    #[test]
    fn test_list_unknown_project() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, None, Some("Nonexistent"), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_ticket_with_empty_title() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(dir.path().join("TD-1.md"), "# TD-1\n* Status: New\n").unwrap();

        run(&config, None, None, false).unwrap();
    }

    #[test]
    fn test_list_includes_done() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(dir.path().join("TD-1 Open.md"), "# TD-1 Open\n* Status: New\n").unwrap();
        let done_dir = dir.path().join("done");
        std::fs::create_dir(&done_dir).unwrap();
        std::fs::write(done_dir.join("TD-2 Closed.md"), "# TD-2 Closed\n* Status: Done\n")
            .unwrap();

        // --all includes done tickets
        run(&config, None, None, true).unwrap();
    }
}
