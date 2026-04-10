// feedback.rs — Search feedback recording

use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from feedback operations.
#[derive(Debug, thiserror::Error)]
pub enum FeedbackError {
    /// A database error occurred.
    #[error("FEEDBACK_DB_ERROR -- {0}")]
    DatabaseError(String),
    /// Invalid action (must be 'accepted' or 'rejected').
    #[error("FEEDBACK_INVALID_ACTION -- {0}")]
    InvalidAction(String),
}

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
        crate::test_db::setup_test_db()
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
