// feedback.rs — Search feedback recording

use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from feedback operations.
#[derive(Debug)]
pub enum FeedbackError {
    /// A database error occurred.
    DatabaseError(String),
    /// Invalid action (must be 'accepted' or 'rejected').
    InvalidAction(String),
}

impl std::fmt::Display for FeedbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FeedbackError::DatabaseError(msg) => {
                write!(f, "FEEDBACK_DB_ERROR -- {}", msg)
            }
            FeedbackError::InvalidAction(msg) => {
                write!(f, "FEEDBACK_INVALID_ACTION -- {}", msg)
            }
        }
    }
}

impl std::error::Error for FeedbackError {}

// ---------------------------------------------------------------------------
// Feedback functions
// ---------------------------------------------------------------------------

/// Record search feedback (accept/reject) for a result.
pub fn record_feedback(
    conn: &Connection,
    project_name: &str,
    file_path: &str,
    file_format: &str,
    action: &str,
    search_type: Option<&str>,
) -> Result<(), FeedbackError> {
    if action != "accepted" && action != "rejected" {
        return Err(FeedbackError::InvalidAction(format!(
            "Invalid action '{}', must be 'accepted' or 'rejected'",
            action
        )));
    }

    conn.execute(
        "INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, search_type)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![project_name, file_path, file_format, action, search_type],
    )
    .map_err(|e| FeedbackError::DatabaseError(e.to_string()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(
            "CREATE TABLE search_feedback (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                file_format TEXT NOT NULL,
                action TEXT NOT NULL CHECK(action IN ('accepted', 'rejected')),
                search_type TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE INDEX idx_feedback_project ON search_feedback(project_name);
            CREATE INDEX idx_feedback_created ON search_feedback(created_at);
            CREATE UNIQUE INDEX idx_feedback_unique_entry ON search_feedback(project_name, file_path);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_record_feedback_accept() {
        let conn = setup_test_db();
        record_feedback(&conn, "film_a", "scene.dpx", "dpx", "accepted", Some("histogram")).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM search_feedback WHERE action = 'accepted'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_record_feedback_reject() {
        let conn = setup_test_db();
        record_feedback(&conn, "film_a", "scene.dpx", "dpx", "rejected", Some("image")).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM search_feedback WHERE action = 'rejected'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_record_feedback_invalid_action() {
        let conn = setup_test_db();
        let result = record_feedback(&conn, "film_a", "scene.dpx", "dpx", "maybe", None);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("FEEDBACK_INVALID_ACTION"));
    }

    #[test]
    fn test_record_feedback_no_search_type() {
        let conn = setup_test_db();
        record_feedback(&conn, "film_a", "scene.dpx", "dpx", "accepted", None).unwrap();

        let search_type: Option<String> = conn
            .query_row(
                "SELECT search_type FROM search_feedback WHERE project_name = 'film_a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(search_type.is_none());
    }

    #[test]
    fn test_feedback_error_display() {
        let err = FeedbackError::DatabaseError("insert failed".to_string());
        assert!(format!("{}", err).contains("FEEDBACK_DB_ERROR"));

        let err = FeedbackError::InvalidAction("bad action".to_string());
        assert!(format!("{}", err).contains("FEEDBACK_INVALID_ACTION"));
    }
}
