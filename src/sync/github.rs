use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};

use super::{CreatedIssue, RemoteComment, RemoteIssue, RemoteState, SyncProvider};

pub struct GitHubProvider {
    pub repo: String,
}

impl GitHubProvider {
    pub fn new(repo: &str) -> Self {
        GitHubProvider {
            repo: repo.to_string(),
        }
    }

    fn run_gh(&self, args: &[&str]) -> Result<String> {
        let output = std::process::Command::new("gh")
            .args(args)
            .output()
            .context("Failed to run `gh` CLI. Is it installed? https://cli.github.com/")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("gh command failed: {}", stderr.trim());
        }
        Ok(String::from_utf8(output.stdout)?)
    }
}

impl SyncProvider for GitHubProvider {
    fn name(&self) -> &str {
        "github"
    }

    fn check_availability(&self) -> Result<()> {
        let output = std::process::Command::new("gh")
            .args(["auth", "status"])
            .output()
            .context("Failed to run `gh` CLI. Is it installed? https://cli.github.com/")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "GitHub CLI is not authenticated. Run `gh auth login` first.\n{}",
                stderr.trim()
            );
        }
        Ok(())
    }

    fn list_issues(&self) -> Result<Vec<RemoteIssue>> {
        let json = self.run_gh(&[
            "issue",
            "list",
            "--repo",
            &self.repo,
            "--state",
            "all",
            "--json",
            "number,title,state,body,updatedAt,createdAt,author",
            "--limit",
            "1000",
        ])?;
        parse_issue_list(&json)
    }

    fn fetch_issue(&self, remote_id: &str) -> Result<RemoteIssue> {
        let json = self.run_gh(&[
            "issue",
            "view",
            remote_id,
            "--repo",
            &self.repo,
            "--json",
            "number,title,state,body,comments,updatedAt,createdAt,author",
        ])?;
        parse_issue_detail(&json)
    }

    fn create_issue(&self, title: &str, body: &str) -> Result<CreatedIssue> {
        let json = self.run_gh(&[
            "issue",
            "create",
            "--repo",
            &self.repo,
            "--title",
            title,
            "--body",
            body,
            "--json",
            "number,updatedAt",
        ])?;
        let v: serde_json::Value = serde_json::from_str(&json)?;
        let number = v["number"]
            .as_u64()
            .context("Missing 'number' in create response")?;
        let updated_at = parse_datetime(
            v["updatedAt"]
                .as_str()
                .context("Missing 'updatedAt' in create response")?,
        )?;
        Ok(CreatedIssue {
            remote_id: number.to_string(),
            updated_at,
        })
    }

    fn add_comment(&self, remote_id: &str, body: &str) -> Result<()> {
        self.run_gh(&[
            "issue",
            "comment",
            remote_id,
            "--repo",
            &self.repo,
            "--body",
            body,
        ])?;
        Ok(())
    }

    fn close_issue(&self, remote_id: &str) -> Result<()> {
        self.run_gh(&["issue", "close", remote_id, "--repo", &self.repo])?;
        Ok(())
    }

    fn reopen_issue(&self, remote_id: &str) -> Result<()> {
        self.run_gh(&["issue", "reopen", remote_id, "--repo", &self.repo])?;
        Ok(())
    }
}

// --- JSON parsing (pure functions, independently testable) ---

#[derive(serde::Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
    state: String,
    body: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    author: GhAuthor,
    comments: Option<Vec<GhComment>>,
}

#[derive(serde::Deserialize)]
struct GhComment {
    author: GhAuthor,
    body: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

#[derive(serde::Deserialize)]
struct GhAuthor {
    login: String,
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .with_context(|| format!("Failed to parse datetime: {}", s))
}

fn gh_issue_to_remote(gh: GhIssue) -> Result<RemoteIssue> {
    let state = match gh.state.as_str() {
        "OPEN" => RemoteState::Open,
        _ => RemoteState::Closed,
    };
    let updated_at = parse_datetime(&gh.updated_at)?;
    let created_at = parse_datetime(&gh.created_at)?;

    let comments = gh
        .comments
        .unwrap_or_default()
        .into_iter()
        .map(|c| {
            Ok(RemoteComment {
                author: c.author.login,
                body: c.body,
                created_at: parse_datetime(&c.created_at)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(RemoteIssue {
        remote_id: gh.number.to_string(),
        title: gh.title,
        state,
        body: gh.body,
        author: gh.author.login,
        created_at,
        comments,
        updated_at,
    })
}

pub fn parse_issue_list(json: &str) -> Result<Vec<RemoteIssue>> {
    let issues: Vec<GhIssue> = serde_json::from_str(json)?;
    issues.into_iter().map(gh_issue_to_remote).collect()
}

pub fn parse_issue_detail(json: &str) -> Result<RemoteIssue> {
    let issue: GhIssue = serde_json::from_str(json)?;
    gh_issue_to_remote(issue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_issue_list() {
        let json = r#"[
            {
                "number": 1,
                "title": "First issue",
                "state": "OPEN",
                "body": "Issue body",
                "updatedAt": "2026-03-30T14:00:00Z",
                "createdAt": "2026-03-29T10:00:00Z",
                "author": {"login": "user1"}
            },
            {
                "number": 2,
                "title": "Closed issue",
                "state": "CLOSED",
                "body": "",
                "updatedAt": "2026-03-30T15:00:00Z",
                "createdAt": "2026-03-28T08:00:00Z",
                "author": {"login": "user2"}
            }
        ]"#;
        let issues = parse_issue_list(json).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].remote_id, "1");
        assert_eq!(issues[0].title, "First issue");
        assert_eq!(issues[0].state, RemoteState::Open);
        assert_eq!(issues[0].author, "user1");
        assert_eq!(issues[1].state, RemoteState::Closed);
        assert!(issues[0].comments.is_empty());
    }

    #[test]
    fn test_parse_issue_detail_with_comments() {
        let json = r#"{
            "number": 42,
            "title": "Feature request",
            "state": "OPEN",
            "body": "Please add this feature.",
            "updatedAt": "2026-03-30T14:00:00Z",
            "createdAt": "2026-03-20T10:00:00Z",
            "author": {"login": "requester"},
            "comments": [
                {
                    "author": {"login": "maintainer"},
                    "body": "Good idea!",
                    "createdAt": "2026-03-21T09:00:00Z"
                },
                {
                    "author": {"login": "requester"},
                    "body": "Thanks!",
                    "createdAt": "2026-03-22T11:00:00Z"
                }
            ]
        }"#;
        let issue = parse_issue_detail(json).unwrap();
        assert_eq!(issue.remote_id, "42");
        assert_eq!(issue.title, "Feature request");
        assert_eq!(issue.body, "Please add this feature.");
        assert_eq!(issue.author, "requester");
        assert_eq!(issue.comments.len(), 2);
        assert_eq!(issue.comments[0].author, "maintainer");
        assert_eq!(issue.comments[0].body, "Good idea!");
        assert_eq!(issue.comments[1].author, "requester");
    }

    #[test]
    fn test_parse_issue_detail_no_comments() {
        let json = r#"{
            "number": 1,
            "title": "Simple",
            "state": "CLOSED",
            "body": "",
            "updatedAt": "2026-01-01T00:00:00Z",
            "createdAt": "2026-01-01T00:00:00Z",
            "author": {"login": "user"},
            "comments": []
        }"#;
        let issue = parse_issue_detail(json).unwrap();
        assert_eq!(issue.state, RemoteState::Closed);
        assert!(issue.body.is_empty());
        assert!(issue.comments.is_empty());
    }

    #[test]
    fn test_parse_issue_list_empty() {
        let json = "[]";
        let issues = parse_issue_list(json).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_issue_list("not json");
        assert!(result.is_err());
    }
}
