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
