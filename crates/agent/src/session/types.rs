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

/// Errors from session operations.
#[derive(Debug)]
pub enum SessionError {
    NotFound(String),
    DbError(String),
    SerializationError(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::NotFound(id) => write!(f, "SESSION_NOT_FOUND -- session '{}' not found", id),
            SessionError::DbError(msg) => write!(f, "SESSION_DB_ERROR -- {}", msg),
            SessionError::SerializationError(msg) => {
                write!(f, "SESSION_SERIALIZATION_ERROR -- {}", msg)
            }
        }
    }
}

impl std::error::Error for SessionError {}
