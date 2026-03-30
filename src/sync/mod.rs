pub mod github;
pub mod metadata;
pub mod orchestrator;

use anyhow::Result;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteState {
    Open,
    Closed,
}

#[derive(Debug, Clone)]
pub struct RemoteComment {
    pub author: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RemoteIssue {
    pub remote_id: String,
    pub title: String,
    pub state: RemoteState,
    pub body: String,
    pub author: String,
    pub created_at: DateTime<Utc>,
    pub comments: Vec<RemoteComment>,
    pub updated_at: DateTime<Utc>,
}

pub struct CreatedIssue {
    pub remote_id: String,
    #[allow(dead_code)]
    pub updated_at: DateTime<Utc>,
}

#[allow(dead_code)]
pub trait SyncProvider {
    fn name(&self) -> &str;
    fn check_availability(&self) -> Result<()>;
    fn list_issues(&self) -> Result<Vec<RemoteIssue>>;
    fn fetch_issue(&self, remote_id: &str) -> Result<RemoteIssue>;
    fn create_issue(&self, title: &str, body: &str) -> Result<CreatedIssue>;
    fn add_comment(&self, remote_id: &str, body: &str) -> Result<()>;
    fn close_issue(&self, remote_id: &str) -> Result<()>;
    fn reopen_issue(&self, remote_id: &str) -> Result<()>;
}
