use chrono::NaiveDateTime;
use std::fmt;

#[derive(Debug, Clone)]
pub struct TicketId {
    pub prefix: String,
    pub number: u32,
}

impl fmt::Display for TicketId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.prefix, self.number)
    }
}

impl TicketId {
    pub fn parse(s: &str) -> Option<Self> {
        let (prefix, rest) = s.split_once('-')?;
        let number: u32 = rest.parse().ok()?;
        if prefix.is_empty()
            || !prefix.starts_with(|c: char| c.is_ascii_uppercase())
            || !prefix.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            return None;
        }
        Some(TicketId {
            prefix: prefix.to_string(),
            number,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Ticket {
    pub id: TicketId,
    pub title: String,
    pub status: Option<String>,
    pub frontmatter: Option<String>,
    pub preamble: String,
    pub comments: Vec<Comment>,
    pub is_closed: bool,
}

#[derive(Debug, Clone)]
pub struct Comment {
    pub author: String,
    pub timestamp: Option<NaiveDateTime>,
    pub body: String,
}

impl TicketId {
    #[cfg(test)]
    pub fn new(prefix: &str, number: u32) -> Self {
        TicketId {
            prefix: prefix.to_string(),
            number,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    // --- TicketId::parse ---

    #[test]
    fn test_parse_valid_id() {
        let id = TicketId::parse("LAW-2").unwrap();
        assert_eq!(id.prefix, "LAW");
        assert_eq!(id.number, 2);
    }

    #[test]
    fn test_parse_id_with_digits_in_prefix() {
        let id = TicketId::parse("SO1-3").unwrap();
        assert_eq!(id.prefix, "SO1");
        assert_eq!(id.number, 3);
    }

    #[test]
    fn test_parse_large_number() {
        let id = TicketId::parse("KMK-22").unwrap();
        assert_eq!(id.prefix, "KMK");
        assert_eq!(id.number, 22);
    }

    #[test]
    fn test_parse_rejects_lowercase_prefix() {
        assert!(TicketId::parse("law-2").is_none());
    }

    #[test]
    fn test_parse_rejects_no_dash() {
        assert!(TicketId::parse("LAW2").is_none());
    }

    #[test]
    fn test_parse_rejects_empty_prefix() {
        assert!(TicketId::parse("-2").is_none());
    }

    #[test]
    fn test_parse_rejects_non_numeric_number() {
        assert!(TicketId::parse("LAW-abc").is_none());
    }

    #[test]
    fn test_parse_rejects_prefix_starting_with_digit() {
        assert!(TicketId::parse("1ABC-2").is_none());
    }

    // --- TicketId::Display ---

    #[test]
    fn test_display() {
        let id = TicketId::new("LAW", 12);
        assert_eq!(format!("{}", id), "LAW-12");
    }

    // --- to_canonical ---

    #[test]
    fn test_canonical_minimal() {
        let ticket = Ticket {
            id: TicketId::new("LAW", 1),
            title: "Test".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        assert_eq!(ticket.to_canonical("New"), "# LAW-1 Test\n* Status: New\n");
    }

    #[test]
    fn test_canonical_empty_title() {
        let ticket = Ticket {
            id: TicketId::new("IDEA", 1),
            title: String::new(),
            status: None,
            frontmatter: None,
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        assert_eq!(ticket.to_canonical("New"), "# IDEA-1\n* Status: New\n");
    }

    #[test]
    fn test_canonical_uses_default_status_when_none() {
        let ticket = Ticket {
            id: TicketId::new("LAW", 1),
            title: "T".to_string(),
            status: None,
            frontmatter: None,
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        let out = ticket.to_canonical("In Progress");
        assert!(out.contains("* Status: In Progress\n"));
    }

    #[test]
    fn test_canonical_preserves_explicit_status() {
        let ticket = Ticket {
            id: TicketId::new("LAW", 1),
            title: "T".to_string(),
            status: Some("Done".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        let out = ticket.to_canonical("New");
        assert!(out.contains("* Status: Done\n"));
    }

    #[test]
    fn test_canonical_with_preamble() {
        let ticket = Ticket {
            id: TicketId::new("IDEA", 1),
            title: String::new(),
            status: None,
            frontmatter: None,
            preamble: "Some old content\nMore lines".to_string(),
            comments: vec![],
            is_closed: false,
        };
        let out = ticket.to_canonical("New");
        assert_eq!(
            out,
            "# IDEA-1\n* Status: New\n\nSome old content\nMore lines\n"
        );
    }

    #[test]
    fn test_canonical_with_comment_and_timestamp() {
        let ts = NaiveDate::from_ymd_opt(2026, 3, 21)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap();
        let ticket = Ticket {
            id: TicketId::new("LAW", 5),
            title: "Feature".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![Comment {
                author: "Lars".to_string(),
                timestamp: Some(ts),
                body: "Implemented the thing.".to_string(),
            }],
            is_closed: false,
        };
        let out = ticket.to_canonical("New");
        assert_eq!(
            out,
            "# LAW-5 Feature\n\
             * Status: New\n\
             \n\
             ## Lars (2026-03-21 14:30)\n\
             Implemented the thing.\n"
        );
    }

    #[test]
    fn test_canonical_comment_without_timestamp() {
        let ticket = Ticket {
            id: TicketId::new("LAW", 1),
            title: "T".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![Comment {
                author: "Meeting".to_string(),
                timestamp: None,
                body: "Notes here.".to_string(),
            }],
            is_closed: false,
        };
        let out = ticket.to_canonical("New");
        assert!(out.contains("## Meeting\nNotes here.\n"));
    }

    #[test]
    fn test_canonical_comment_with_empty_body() {
        let ts = NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let ticket = Ticket {
            id: TicketId::new("LAW", 1),
            title: "T".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![Comment {
                author: "Lars".to_string(),
                timestamp: Some(ts),
                body: String::new(),
            }],
            is_closed: false,
        };
        let out = ticket.to_canonical("New");
        assert_eq!(
            out,
            "# LAW-1 T\n* Status: New\n\n## Lars (2026-01-01 00:00)\n"
        );
    }

    #[test]
    fn test_canonical_multiple_comments() {
        let ts1 = NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let ts2 = NaiveDate::from_ymd_opt(2026, 1, 2)
            .unwrap()
            .and_hms_opt(11, 0, 0)
            .unwrap();
        let ticket = Ticket {
            id: TicketId::new("LAW", 1),
            title: "T".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![
                Comment {
                    author: "A".to_string(),
                    timestamp: Some(ts1),
                    body: "First.".to_string(),
                },
                Comment {
                    author: "B".to_string(),
                    timestamp: Some(ts2),
                    body: "Second.".to_string(),
                },
            ],
            is_closed: false,
        };
        let out = ticket.to_canonical("New");
        assert!(out.contains("## A (2026-01-01 10:00)\nFirst.\n\n## B (2026-01-02 11:00)\nSecond.\n"));
    }

    #[test]
    fn test_canonical_with_frontmatter() {
        let ticket = Ticket {
            id: TicketId::new("LAW", 1),
            title: "Test".to_string(),
            status: Some("New".to_string()),
            frontmatter: Some("source: github\nrepo: https://github.com/user/repo".to_string()),
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        assert_eq!(
            ticket.to_canonical("New"),
            "---\nsource: github\nrepo: https://github.com/user/repo\n---\n# LAW-1 Test\n* Status: New\n"
        );
    }

    #[test]
    fn test_canonical_with_empty_frontmatter() {
        let ticket = Ticket {
            id: TicketId::new("LAW", 1),
            title: "Test".to_string(),
            status: Some("New".to_string()),
            frontmatter: Some(String::new()),
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        assert_eq!(
            ticket.to_canonical("New"),
            "---\n---\n# LAW-1 Test\n* Status: New\n"
        );
    }

    #[test]
    fn test_canonical_without_frontmatter_unchanged() {
        let ticket = Ticket {
            id: TicketId::new("LAW", 1),
            title: "Test".to_string(),
            status: Some("New".to_string()),
            frontmatter: None,
            preamble: String::new(),
            comments: vec![],
            is_closed: false,
        };
        assert_eq!(ticket.to_canonical("New"), "# LAW-1 Test\n* Status: New\n");
    }
}

impl Ticket {
    pub fn to_canonical(&self, default_status: &str) -> String {
        let mut out = String::new();

        // Frontmatter
        if let Some(ref fm) = self.frontmatter {
            out.push_str("---\n");
            out.push_str(fm);
            if !fm.is_empty() && !fm.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("---\n");
        }

        // Heading
        if self.title.is_empty() {
            out.push_str(&format!("# {}\n", self.id));
        } else {
            out.push_str(&format!("# {} {}\n", self.id, self.title));
        }

        // Status
        let status = self.status.as_deref().unwrap_or(default_status);
        out.push_str(&format!("* Status: {}\n", status));

        // Preamble (content before first comment that isn't heading/status)
        if !self.preamble.is_empty() {
            out.push('\n');
            out.push_str(&self.preamble);
            out.push('\n');
        }

        // Comments
        for comment in &self.comments {
            out.push('\n');
            match comment.timestamp {
                Some(ts) => {
                    out.push_str(&format!(
                        "## {} ({})\n",
                        comment.author,
                        ts.format("%Y-%m-%d %H:%M")
                    ));
                }
                None => {
                    out.push_str(&format!("## {}\n", comment.author));
                }
            }
            if !comment.body.is_empty() {
                out.push_str(&comment.body);
                out.push('\n');
            }
        }

        out
    }
}
