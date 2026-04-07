use anyhow::Result;

use crate::commands::list::print_ticket_row;
use crate::config::Config;
use crate::store::TicketStore;
use crate::ticket::Ticket;

pub fn run(config: &Config, query: &str, all: bool) -> Result<()> {
    let matches = matching_tickets(config, query, all)?;

    if matches.is_empty() {
        println!("No tickets found.");
        return Ok(());
    }

    for ticket in &matches {
        print_ticket_row(ticket);
    }

    Ok(())
}

/// Load all tickets and return only those matching the substring query
/// (case-insensitive) in title, preamble, or any comment body.
fn matching_tickets(config: &Config, query: &str, all: bool) -> Result<Vec<Ticket>> {
    let store = TicketStore::new(&config.notes_dir);
    let tickets = store.scan_tickets(all)?;
    let needle = query.to_lowercase();
    Ok(tickets.into_iter().filter(|t| ticket_matches(t, &needle)).collect())
}

fn ticket_matches(ticket: &Ticket, needle_lower: &str) -> bool {
    if ticket.title.to_lowercase().contains(needle_lower) {
        return true;
    }
    if ticket.preamble.to_lowercase().contains(needle_lower) {
        return true;
    }
    ticket
        .comments
        .iter()
        .any(|c| c.body.to_lowercase().contains(needle_lower))
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
    fn test_search_empty_dir_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        run(&config, "anything", false).unwrap();
    }

    #[test]
    fn test_search_matches_title() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        std::fs::write(
            dir.path().join("TD-1 Implement search command.md"),
            "# TD-1 Implement search command\n* Status: New\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("TD-2 Unrelated thing.md"),
            "# TD-2 Unrelated thing\n* Status: New\n",
        )
        .unwrap();

        let matches = matching_tickets(&config, "search", false).unwrap();
        let ids: Vec<String> = matches.iter().map(|t| t.id.to_string()).collect();
        assert_eq!(ids, vec!["TD-1".to_string()]);
    }

    #[test]
    fn test_search_matches_preamble() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        std::fs::write(
            dir.path().join("TD-1 Plain.md"),
            "# TD-1 Plain\n* Status: New\n\nNotes about the rhubarb pie recipe.\n",
        )
        .unwrap();

        let matches = matching_tickets(&config, "rhubarb", false).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id.to_string(), "TD-1");
    }

    #[test]
    fn test_search_matches_comment_body() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        std::fs::write(
            dir.path().join("TD-1 Plain.md"),
            "# TD-1 Plain\n* Status: New\n\n## Lars (2026-01-01 10:00)\nDiscovered an off-by-one bug.\n",
        )
        .unwrap();

        let matches = matching_tickets(&config, "off-by-one", false).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id.to_string(), "TD-1");
    }

    #[test]
    fn test_search_is_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        std::fs::write(
            dir.path().join("TD-1 FooBar.md"),
            "# TD-1 FooBar\n* Status: New\n",
        )
        .unwrap();

        let matches = matching_tickets(&config, "foobar", false).unwrap();
        assert_eq!(matches.len(), 1);
        let matches = matching_tickets(&config, "FOO", false).unwrap();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_search_no_match_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        std::fs::write(
            dir.path().join("TD-1 Apple.md"),
            "# TD-1 Apple\n* Status: New\n",
        )
        .unwrap();

        let matches = matching_tickets(&config, "banana", false).unwrap();
        assert!(matches.is_empty());
        // run() should still succeed (prints "No tickets found.")
        run(&config, "banana", false).unwrap();
    }

    #[test]
    fn test_search_excludes_closed_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let done_dir = dir.path().join("done");
        std::fs::create_dir(&done_dir).unwrap();
        std::fs::write(
            done_dir.join("TD-1 Closed match.md"),
            "# TD-1 Closed match\n* Status: Done\n",
        )
        .unwrap();

        let matches = matching_tickets(&config, "match", false).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_search_all_includes_closed() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let done_dir = dir.path().join("done");
        std::fs::create_dir(&done_dir).unwrap();
        std::fs::write(
            done_dir.join("TD-1 Closed match.md"),
            "# TD-1 Closed match\n* Status: Done\n",
        )
        .unwrap();

        let matches = matching_tickets(&config, "match", true).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id.to_string(), "TD-1");
    }
}
