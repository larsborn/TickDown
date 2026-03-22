use chrono::NaiveDateTime;
use regex::Regex;

use crate::ticket::{Comment, Ticket, TicketId};

/// Parse a filename stem (without .md extension) into a TicketId and title.
pub fn parse_filename(stem: &str) -> Option<(TicketId, String)> {
    let re = Regex::new(r"^([A-Z][A-Z0-9]*)-(\d+)[\s\-]?(.*)$").unwrap();
    let caps = re.captures(stem)?;
    let prefix = caps[1].to_string();
    let number: u32 = caps[2].parse().ok()?;
    let title = caps[3].trim().to_string();
    Some((TicketId { prefix, number }, title))
}

/// Parse ticket file content into a Ticket struct. Never errors on format issues.
pub fn parse_ticket(id: TicketId, filename_title: &str, content: &str, is_closed: bool) -> Ticket {
    let comment_re =
        Regex::new(r"^\s*##\s+(.+?)\s*(?:\((\d{4}-\d{2}-\d{2})(?:\s+(\d{2}:\d{2}))?\))?\s*$")
            .unwrap();
    let status_re = Regex::new(r"^\*\s*[Ss]tatus:\s*(.+)$").unwrap();

    let mut status: Option<String> = None;
    let mut heading_title: Option<String> = None;
    let mut preamble_lines: Vec<String> = Vec::new();
    let mut comments: Vec<Comment> = Vec::new();
    let mut current_comment: Option<(String, Option<NaiveDateTime>, Vec<String>)> = None;
    let mut in_header = true;

    for line in content.lines() {
        // Check for comment header
        if let Some(caps) = comment_re.captures(line) {
            // Flush previous comment
            if let Some((author, ts, body_lines)) = current_comment.take() {
                let body = body_lines.join("\n");
                comments.push(Comment {
                    author,
                    timestamp: ts,
                    body: body.trim().to_string(),
                });
            }
            in_header = false;
            let author = caps[1].to_string();
            let timestamp = parse_comment_timestamp(&caps);
            current_comment = Some((author, timestamp, Vec::new()));
            continue;
        }

        if in_header {
            let trimmed = line.trim();

            // Heading line (# but not ##)
            if trimmed.starts_with("# ") && !trimmed.starts_with("## ") {
                let heading_text = trimmed.trim_start_matches("# ");
                heading_title = Some(clean_heading_title(heading_text));
                continue;
            }

            // Status line
            if let Some(caps) = status_re.captures(trimmed) {
                status = Some(caps[1].trim().to_string());
                continue;
            }

            // Skip blank lines in header area
            if trimmed.is_empty() {
                continue;
            }

            // Non-heading, non-status content -> preamble
            preamble_lines.push(line.to_string());
        } else if let Some(ref mut comment) = current_comment {
            comment.2.push(line.to_string());
        }
    }

    // Flush last comment
    if let Some((author, ts, body_lines)) = current_comment.take() {
        let body = body_lines.join("\n");
        comments.push(Comment {
            author,
            timestamp: ts,
            body: body.trim().to_string(),
        });
    }

    // Use heading title if available, otherwise filename title
    let title = heading_title.unwrap_or_else(|| filename_title.to_string());

    Ticket {
        id,
        title,
        status,
        preamble: preamble_lines.join("\n").trim().to_string(),
        comments,
        is_closed,
    }
}

/// Remove ticket ID prefix and [Project] tags from heading text.
fn clean_heading_title(heading: &str) -> String {
    let mut s = heading.to_string();

    // Remove any leading ticket ID prefix (e.g., "LAW-2 " or "LAW-3 ")
    let id_re = Regex::new(r"^[A-Z][A-Z0-9]*-\d+\s*").unwrap();
    s = id_re.replace(&s, "").to_string();

    // Remove [Project] tags
    let tag_re = Regex::new(r"\[.*?\]\s*").unwrap();
    s = tag_re.replace_all(&s, "").to_string();

    s.trim().to_string()
}

fn parse_comment_timestamp(caps: &regex::Captures) -> Option<NaiveDateTime> {
    let date_str = caps.get(2)?.as_str();
    let time_str = caps.get(3).map(|m| m.as_str()).unwrap_or("00:00");
    let datetime_str = format!("{} {}", date_str, time_str);
    NaiveDateTime::parse_from_str(&datetime_str, "%Y-%m-%d %H:%M").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_filename_standard() {
        let (id, title) = parse_filename("LAW-2 TickDown Command Line Client Prototype").unwrap();
        assert_eq!(id.prefix, "LAW");
        assert_eq!(id.number, 2);
        assert_eq!(title, "TickDown Command Line Client Prototype");
    }

    #[test]
    fn test_parse_filename_dash_separator() {
        let (id, title) = parse_filename("LAW-4-Something").unwrap();
        assert_eq!(id.prefix, "LAW");
        assert_eq!(id.number, 4);
        assert_eq!(title, "Something");
    }

    #[test]
    fn test_parse_filename_no_title() {
        let (id, title) = parse_filename("BLK-1").unwrap();
        assert_eq!(id.prefix, "BLK");
        assert_eq!(id.number, 1);
        assert_eq!(title, "");
    }

    #[test]
    fn test_parse_filename_zero_padded() {
        let (id, title) = parse_filename("IDEA-001").unwrap();
        assert_eq!(id.prefix, "IDEA");
        assert_eq!(id.number, 1);
        assert_eq!(title, "");
    }

    #[test]
    fn test_parse_filename_double_space() {
        let (id, title) = parse_filename("FOO-1  VB").unwrap();
        assert_eq!(id.prefix, "FOO");
        assert_eq!(id.number, 1);
        assert_eq!(title, "VB");
    }

    #[test]
    fn test_parse_ticket_standard() {
        let content = "# LAW-2 [TickDown] Command Line Client Prototype\n\
                        * Status: In Progress\n\
                        \n\
                        ## Lars Wallenborn (2019-03-16 20:45)\n\
                        Some comment body.\n";
        let id = TicketId {
            prefix: "LAW".into(),
            number: 2,
        };
        let ticket = parse_ticket(id, "TickDown Command Line Client Prototype", content, false);
        assert_eq!(ticket.title, "Command Line Client Prototype");
        assert_eq!(ticket.status.as_deref(), Some("In Progress"));
        assert_eq!(ticket.comments.len(), 1);
        assert_eq!(ticket.comments[0].author, "Lars Wallenborn");
        assert!(ticket.comments[0].timestamp.is_some());
        assert_eq!(ticket.comments[0].body, "Some comment body.");
    }

    #[test]
    fn test_parse_ticket_no_heading() {
        let content = "Just some text\n* more text\n";
        let id = TicketId {
            prefix: "IDEA".into(),
            number: 1,
        };
        let ticket = parse_ticket(id, "", content, false);
        assert_eq!(ticket.title, "");
        assert_eq!(ticket.preamble, "Just some text\n* more text");
    }

    #[test]
    fn test_parse_ticket_date_without_time() {
        let content = "# GTN-5 [Garten] Bambushecke\n\
                        \n\
                        ## Karl Heinz (2020-06-13)\n\
                        * Some notes\n";
        let id = TicketId {
            prefix: "GTN".into(),
            number: 5,
        };
        let ticket = parse_ticket(id, "Baumbushecke", content, false);
        assert_eq!(ticket.title, "Bambushecke");
        assert_eq!(ticket.comments.len(), 1);
        let ts = ticket.comments[0].timestamp.unwrap();
        assert_eq!(ts.format("%H:%M").to_string(), "00:00");
    }

    #[test]
    fn test_parse_ticket_heading_id_mismatch() {
        let content = "# LAW-3 [Something] Data\n\
                        * Status: Waiting For\n";
        let id = TicketId {
            prefix: "LAW".into(),
            number: 4,
        };
        let ticket = parse_ticket(id, "Something", content, false);
        // Heading title is used (with tags and wrong ID removed)
        assert_eq!(ticket.title, "Data");
        assert_eq!(ticket.id.number, 4); // filename ID is authoritative
    }

    #[test]
    fn test_parse_ticket_multiple_comments() {
        let content = "# LAW-1 Title\n\
                        * Status: New\n\
                        \n\
                        ## Alice (2026-01-01 10:00)\n\
                        First comment.\n\
                        \n\
                        ## Bob (2026-01-02 11:00)\n\
                        Second comment.\n";
        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let ticket = parse_ticket(id, "Title", content, false);
        assert_eq!(ticket.comments.len(), 2);
        assert_eq!(ticket.comments[0].author, "Alice");
        assert_eq!(ticket.comments[0].body, "First comment.");
        assert_eq!(ticket.comments[1].author, "Bob");
        assert_eq!(ticket.comments[1].body, "Second comment.");
    }

    #[test]
    fn test_parse_ticket_leading_whitespace_on_comment_header() {
        let content = "# LAW-1 Title\n\
                        * Status: New\n\
                        \n\
                         ## Lars (2026-01-01 10:00)\n\
                        Body text.\n";
        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let ticket = parse_ticket(id, "Title", content, false);
        assert_eq!(ticket.comments.len(), 1);
        assert_eq!(ticket.comments[0].author, "Lars");
        assert_eq!(ticket.comments[0].body, "Body text.");
    }

    #[test]
    fn test_parse_ticket_multi_author_comment() {
        let content = "# LAW-1 Title\n\
                        \n\
                        ## Alice / Bob (2026-01-01 10:00)\n\
                        Joint notes.\n";
        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let ticket = parse_ticket(id, "Title", content, false);
        assert_eq!(ticket.comments[0].author, "Alice / Bob");
    }

    #[test]
    fn test_parse_ticket_comment_without_date() {
        let content = "# BLK-1 Title\n\
                        \n\
                        ## ToDo\n\
                        * Item one\n\
                        * Item two\n";
        let id = TicketId {
            prefix: "BLK".into(),
            number: 1,
        };
        let ticket = parse_ticket(id, "Title", content, false);
        assert_eq!(ticket.comments.len(), 1);
        assert_eq!(ticket.comments[0].author, "ToDo");
        assert!(ticket.comments[0].timestamp.is_none());
        assert_eq!(ticket.comments[0].body, "* Item one\n* Item two");
    }

    #[test]
    fn test_parse_ticket_is_closed_flag() {
        let content = "# LAW-1 Title\n* Status: Done\n";
        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let ticket = parse_ticket(id, "Title", content, true);
        assert!(ticket.is_closed);
    }

    #[test]
    fn test_parse_ticket_missing_status() {
        let content = "# LAW-1 Title\n\n## Author (2026-01-01)\nBody.\n";
        let id = TicketId {
            prefix: "LAW".into(),
            number: 1,
        };
        let ticket = parse_ticket(id, "Title", content, false);
        assert!(ticket.status.is_none());
    }

    #[test]
    fn test_parse_ticket_roundtrip() {
        let content = "# LAW-5 Feature\n\
                        * Status: In Progress\n\
                        \n\
                        ## Lars (2026-03-21 14:30)\n\
                        Did the thing.\n\
                        \n\
                        ## Lars (2026-03-22 09:00)\n\
                        Did more.\n";
        let id = TicketId {
            prefix: "LAW".into(),
            number: 5,
        };
        let ticket = parse_ticket(id, "Feature", content, false);
        let canonical = ticket.to_canonical("New");
        assert_eq!(canonical, content);
    }

    #[test]
    fn test_parse_filename_rejects_lowercase() {
        assert!(parse_filename("low-1 Title").is_none());
    }

    #[test]
    fn test_clean_heading_title() {
        assert_eq!(
            clean_heading_title("LAW-2 [TickDown] Command Line Client Prototype"),
            "Command Line Client Prototype"
        );
        assert_eq!(
            clean_heading_title("GTN-5 [Garten] Bambushecke"),
            "Bambushecke"
        );
        assert_eq!(
            clean_heading_title("nullteilerfrei.de"),
            "nullteilerfrei.de"
        );
    }
}
