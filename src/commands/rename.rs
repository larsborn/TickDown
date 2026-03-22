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
