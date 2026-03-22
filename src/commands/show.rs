use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::store::TicketStore;
use crate::ticket::TicketId;

pub fn run(config: &Config, ticket_id: &str) -> Result<()> {
    let id = TicketId::parse(ticket_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid ticket ID: {}", ticket_id))?;

    let store = TicketStore::new(&config.notes_dir);
    let ticket = store.read_ticket(&id)?;

    // Header
    let header = if ticket.title.is_empty() {
        format!("{}", ticket.id)
    } else {
        format!("{} {}", ticket.id, ticket.title)
    };
    println!("{}", header.bold());

    // Status
    let status = ticket.status.as_deref().unwrap_or("(no status)");
    let status_colored = match status {
        "Done" => status.green(),
        "In Progress" => status.yellow(),
        "Waiting For" => status.cyan(),
        _ => status.white(),
    };
    println!("Status: {}", status_colored);

    if ticket.is_closed {
        println!("{}", "(closed)".dimmed());
    }

    // Preamble
    if !ticket.preamble.is_empty() {
        println!();
        println!("{}", ticket.preamble);
    }

    // Comments
    for comment in &ticket.comments {
        println!();
        let header = if let Some(ts) = comment.timestamp {
            format!("{} ({})", comment.author, ts.format("%Y-%m-%d %H:%M"))
        } else {
            comment.author.clone()
        };
        println!("{}", header.blue().bold());
        if !comment.body.is_empty() {
            println!("{}", comment.body);
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
    fn test_show_ticket() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(
            dir.path().join("TD-1 Test.md"),
            "# TD-1 Test\n* Status: In Progress\n\n## Author (2026-01-01 10:00)\nComment body.\n",
        )
        .unwrap();

        run(&config, "TD-1").unwrap();
    }

    #[test]
    fn test_show_ticket_no_status() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(dir.path().join("TD-1 Test.md"), "# TD-1 Test\n").unwrap();

        run(&config, "TD-1").unwrap();
    }

    #[test]
    fn test_show_ticket_with_preamble() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        std::fs::write(
            dir.path().join("TD-1 Test.md"),
            "# TD-1 Test\n* Status: New\n\nSome preamble text.\n\n## Author (2026-01-01 10:00)\nBody.\n",
        )
        .unwrap();

        run(&config, "TD-1").unwrap();
    }

    #[test]
    fn test_show_closed_ticket() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        let done_dir = dir.path().join("done");
        std::fs::create_dir(&done_dir).unwrap();
        std::fs::write(
            done_dir.join("TD-1 Closed.md"),
            "# TD-1 Closed\n* Status: Done\n",
        )
        .unwrap();

        run(&config, "TD-1").unwrap();
    }

    #[test]
    fn test_show_invalid_id() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "bad");
        assert!(result.is_err());
    }

    #[test]
    fn test_show_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let result = run(&config, "TD-99");
        assert!(result.is_err());
    }
}
