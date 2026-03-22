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
