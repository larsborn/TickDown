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
