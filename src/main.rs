mod commands;
mod config;
mod parser;
mod store;
mod ticket;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[cfg(windows)]
fn enable_ansi_support() {
    use std::os::windows::io::AsRawHandle;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    unsafe {
        let handle = std::io::stdout().as_raw_handle();
        let mut mode: u32 = 0;
        extern "system" {
            fn GetConsoleMode(h: *mut std::ffi::c_void, m: *mut u32) -> i32;
            fn SetConsoleMode(h: *mut std::ffi::c_void, m: u32) -> i32;
        }
        if GetConsoleMode(handle, &mut mode) != 0 {
            let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

#[cfg(not(windows))]
fn enable_ansi_support() {}

#[derive(Parser)]
#[command(name = "td", about = "TickDown - Markdown ticket manager")]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new ticket
    Create {
        /// Project name (mapped to prefix in config)
        project: String,
        /// Ticket title
        title: String,
    },
    /// Pretty-print a ticket
    Show {
        /// Ticket ID (e.g., LAW-2)
        ticket_id: String,
    },
    /// List tickets
    #[command(alias = "ls")]
    List {
        /// Filter by prefix
        #[arg(long)]
        prefix: Option<String>,
        /// Filter by project name
        #[arg(long)]
        project: Option<String>,
        /// Include closed tickets from done/
        #[arg(long)]
        all: bool,
    },
    /// Open ticket in $EDITOR
    Edit {
        /// Ticket ID (e.g., LAW-2)
        ticket_id: String,
    },
    /// Add a comment to a ticket (normalizes file)
    Comment {
        /// Ticket ID (e.g., LAW-2)
        ticket_id: String,
        /// Comment text
        text: String,
    },
    /// Move ticket to done/
    #[command(alias = "done")]
    Close {
        /// Ticket ID (e.g., LAW-2)
        ticket_id: String,
    },
    /// Rename a ticket's title
    #[command(alias = "mv")]
    Rename {
        /// Ticket ID (e.g., LAW-2)
        ticket_id: String,
        /// New title
        new_title: String,
    },
    /// Move a ticket to a different project/prefix
    #[command(alias = "mod")]
    Modify {
        /// Ticket ID (e.g., LAW-2)
        ticket_id: String,
        /// Target project (by name, e.g., "Garten")
        #[arg(long)]
        project: Option<String>,
        /// Target prefix directly (e.g., "CP")
        #[arg(long)]
        prefix: Option<String>,
    },
    /// Rename an entire project (name and/or prefix)
    RenameProject {
        /// Current project name or prefix
        current: String,
        /// New project name
        #[arg(long)]
        name: Option<String>,
        /// New prefix (renames all ticket files)
        #[arg(long)]
        prefix: Option<String>,
    },
}

fn main() -> Result<()> {
    enable_ansi_support();
    let cli = Cli::parse();
    let config = config::load_config(cli.config.as_deref())?;

    match cli.command {
        Commands::Create { project, title } => commands::create::run(&config, &project, &title),
        Commands::Show { ticket_id } => commands::show::run(&config, &ticket_id),
        Commands::List {
            prefix,
            project,
            all,
        } => commands::list::run(&config, prefix.as_deref(), project.as_deref(), all),
        Commands::Edit { ticket_id } => commands::edit::run(&config, &ticket_id),
        Commands::Comment { ticket_id, text } => {
            commands::comment::run(&config, &ticket_id, &text)
        }
        Commands::Close { ticket_id } => commands::close::run(&config, &ticket_id),
        Commands::Rename {
            ticket_id,
            new_title,
        } => commands::rename::run(&config, &ticket_id, &new_title),
        Commands::Modify {
            ticket_id,
            project,
            prefix,
        } => commands::modify::run(
            &config,
            &ticket_id,
            project.as_deref(),
            prefix.as_deref(),
        ),
        Commands::RenameProject {
            current,
            name,
            prefix,
        } => commands::rename_project::run(
            &config,
            &current,
            name.as_deref(),
            prefix.as_deref(),
        ),
    }
}
