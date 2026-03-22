use anyhow::Result;

use crate::config::Config;
use crate::store::TicketStore;
use crate::ticket::TicketId;

pub fn run(config: &Config, ticket_id: &str) -> Result<()> {
    let id = TicketId::parse(ticket_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid ticket ID: {}", ticket_id))?;

    let store = TicketStore::new(&config.notes_dir);
    let path = store.find_ticket_file(&id)?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".to_string());
    let status = std::process::Command::new(&editor).arg(&path).status()?;

    if !status.success() {
        anyhow::bail!("Editor exited with status: {}", status);
    }

    Ok(())
}
