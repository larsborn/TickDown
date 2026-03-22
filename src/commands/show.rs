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
