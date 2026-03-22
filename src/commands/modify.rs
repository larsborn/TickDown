use anyhow::{bail, Result};

use crate::config::Config;
use crate::store::TicketStore;
use crate::ticket::TicketId;

pub fn run(
    config: &Config,
    ticket_id: &str,
    new_project: Option<&str>,
    new_prefix: Option<&str>,
) -> Result<()> {
    if new_project.is_none() && new_prefix.is_none() {
        bail!("Provide --project or --prefix.");
    }
    if new_project.is_some() && new_prefix.is_some() {
        bail!("Provide either --project or --prefix, not both.");
    }

    let id = TicketId::parse(ticket_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid ticket ID: {}", ticket_id))?;

    let input = new_project.or(new_prefix).unwrap();
    let prefix = config.resolve_prefix(input).ok_or_else(|| {
        let known: Vec<_> = config
            .projects
            .iter()
            .map(|(name, pfx)| format!("{} ({})", name, pfx))
            .collect();
        anyhow::anyhow!(
            "Unknown project or prefix: {}. Known: {}",
            input,
            known.join(", ")
        )
    })?;

    let store = TicketStore::new(&config.notes_dir);
    let old_path = store.find_ticket_file(&id)?;
    let mut ticket = store.read_ticket(&id)?;

    if prefix == ticket.id.prefix {
        println!("{} is already in prefix {}", ticket.id, prefix);
        return Ok(());
    }

    let number = store.next_number(&prefix)?;
    let old_id = ticket.id.clone();
    ticket.id = TicketId { prefix, number };

    let new_path = store.write_ticket(&ticket, &config.statuses.default)?;

    if old_path != new_path && old_path.exists() {
        std::fs::remove_file(&old_path)?;
    }

    println!("Moved {} -> {} ({})", old_id, ticket.id, new_path.display());
    Ok(())
}
