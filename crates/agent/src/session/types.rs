// session/types.rs — Session data types

/// A persisted agent session.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Summary of a session for listing purposes.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub message_count: i64,
    pub updated_at: i64,
}

/// A branch within a session's conversation tree.
#[derive(Debug, Clone)]
pub struct Branch {
    pub id: String,
    pub session_id: String,
    pub parent_branch_id: Option<String>,
    pub branch_point_seq: Option<i64>,
    pub name: String,
    pub created_at: i64,
}

/// Tree node for branch navigation in the UI.
#[derive(Debug, Clone)]
pub struct BranchTreeNode {
    pub branch: Branch,
    pub message_count: i64,
    pub children: Vec<BranchTreeNode>,
}

/// Result of a compaction operation.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    pub messages_compacted: usize,
    pub summary_seq: i64,
    pub messages_preserved: usize,
}

/// Errors from session operations.
#[derive(Debug)]
pub enum SessionError {
    NotFound(String),
    DbError(String),
    SerializationError(String),
    BranchNotFound(String),
    InvalidBranchPoint(String),
    CompactionError(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::NotFound(id) => write!(f, "SESSION_NOT_FOUND -- session '{}' not found", id),
            SessionError::DbError(msg) => write!(f, "SESSION_DB_ERROR -- {}", msg),
            SessionError::SerializationError(msg) => {
                write!(f, "SESSION_SERIALIZATION_ERROR -- {}", msg)
            }
            SessionError::BranchNotFound(id) => {
                write!(f, "SESSION_BRANCH_NOT_FOUND -- branch '{}' not found", id)
            }
            SessionError::InvalidBranchPoint(msg) => {
                write!(f, "SESSION_INVALID_BRANCH_POINT -- {}", msg)
            }
            SessionError::CompactionError(msg) => {
                write!(f, "SESSION_COMPACTION_ERROR -- {}", msg)
            }
        }
    }
}

impl std::error::Error for SessionError {}
