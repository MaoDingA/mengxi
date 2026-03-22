// analytics.rs — Session tracking and usage analytics

use rusqlite::{Connection, OptionalExtension};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from analytics operations.
#[derive(Debug)]
pub enum AnalyticsError {
    /// A database error occurred.
    DatabaseError(String),
}

impl std::fmt::Display for AnalyticsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalyticsError::DatabaseError(msg) => {
                write!(f, "ANALYTICS_DB_ERROR -- {}", msg)
            }
        }
    }
}

impl std::error::Error for AnalyticsError {}

// ---------------------------------------------------------------------------
// Session record
// ---------------------------------------------------------------------------

/// A recorded CLI session.
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub command: String,
    pub args_json: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration_ms: i64,
    pub exit_code: i32,
    pub search_to_export_ms: Option<i64>,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Record a session.
pub fn record_session(
    conn: &Connection,
    session_id: &str,
    command: &str,
    args_json: &str,
    started_at: i64,
    ended_at: i64,
    duration_ms: i64,
    exit_code: i32,
    search_to_export_ms: Option<i64>,
) -> Result<(), AnalyticsError> {
    conn.execute(
        "INSERT OR REPLACE INTO analytics_sessions (session_id, command, args_json, started_at, ended_at, duration_ms, exit_code, search_to_export_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            session_id,
            command,
            args_json,
            started_at,
            ended_at,
            duration_ms,
            exit_code,
            search_to_export_ms,
        ],
    )
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(())
}

/// Get the started_at timestamp of the most recent search session.
pub fn get_last_search_started_at(conn: &Connection) -> Result<Option<i64>, AnalyticsError> {
    let mut stmt = conn
        .prepare(
            "SELECT started_at FROM analytics_sessions WHERE command = 'search' ORDER BY started_at DESC LIMIT 1",
        )
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let result = stmt
        .query_row([], |row| row.get(0))
        .optional()
        .map_err(|e: rusqlite::Error| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(result.flatten())
}

/// Get session count within a time range.
pub fn get_session_count(conn: &Connection, since_timestamp: Option<i64>) -> Result<usize, AnalyticsError> {
    let count: i64 = match since_timestamp {
        Some(since) => conn
            .query_row(
                "SELECT COUNT(*) FROM analytics_sessions WHERE started_at >= ?1",
                rusqlite::params![since],
                |row| row.get(0),
            ),
        None => conn
            .query_row(
                "SELECT COUNT(*) FROM analytics_sessions",
                rusqlite::params![],
                |row| row.get(0),
            ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(count as usize)
}

/// Get average session duration in milliseconds within a time range.
pub fn get_average_duration_ms(conn: &Connection, since_timestamp: Option<i64>) -> Result<i64, AnalyticsError> {
    let avg: f64 = match since_timestamp {
        Some(since) => conn
            .query_row(
                "SELECT COALESCE(AVG(duration_ms), 0) FROM analytics_sessions WHERE started_at >= ?1 AND exit_code = 0",
                rusqlite::params![since],
                |row| row.get(0),
            ),
        None => conn
            .query_row(
                "SELECT COALESCE(AVG(duration_ms), 0) FROM analytics_sessions WHERE exit_code = 0",
                rusqlite::params![],
                |row| row.get(0),
            ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(avg as i64)
}

/// Get command breakdown (command -> count) within a time range.
pub fn get_command_breakdown(
    conn: &Connection,
    since_timestamp: Option<i64>,
) -> Result<Vec<(String, usize)>, AnalyticsError> {
    let mut stmt = match since_timestamp {
        Some(_) => conn.prepare(
            "SELECT command, COUNT(*) as cnt FROM analytics_sessions WHERE started_at >= ?1 GROUP BY command ORDER BY cnt DESC",
        ),
        None => conn.prepare(
            "SELECT command, COUNT(*) as cnt FROM analytics_sessions GROUP BY command ORDER BY cnt DESC",
        ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let rows: Vec<(String, usize)> = match since_timestamp {
        Some(since) => stmt
            .query_map(rusqlite::params![since], |row| {
                let command: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((command, count as usize))
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
        None => stmt
            .query_map(rusqlite::params![], |row| {
                let command: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((command, count as usize))
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
    };

    Ok(rows)
}

/// Get recent sessions within a time range, ordered by most recent first.
pub fn get_sessions(
    conn: &Connection,
    since_timestamp: Option<i64>,
    limit: usize,
) -> Result<Vec<SessionRecord>, AnalyticsError> {
    let mut stmt = match since_timestamp {
        Some(_) => conn.prepare(
            "SELECT session_id, command, args_json, started_at, ended_at, duration_ms, exit_code, search_to_export_ms
             FROM analytics_sessions WHERE started_at >= ?1
             ORDER BY started_at DESC LIMIT ?2",
        ),
        None => conn.prepare(
            "SELECT session_id, command, args_json, started_at, ended_at, duration_ms, exit_code, search_to_export_ms
             FROM analytics_sessions ORDER BY started_at DESC LIMIT ?1",
        ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let rows: Vec<SessionRecord> = match since_timestamp {
        Some(since) => stmt
            .query_map(rusqlite::params![since, limit as i64], |row| {
                Ok(SessionRecord {
                    session_id: row.get(0)?,
                    command: row.get(1)?,
                    args_json: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    duration_ms: row.get(5)?,
                    exit_code: row.get(6)?,
                    search_to_export_ms: row.get(7)?,
                })
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
        None => stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok(SessionRecord {
                    session_id: row.get(0)?,
                    command: row.get(1)?,
                    args_json: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    duration_ms: row.get(5)?,
                    exit_code: row.get(6)?,
                    search_to_export_ms: row.get(7)?,
                })
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
    };

    Ok(rows)
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
            "CREATE TABLE analytics_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                command TEXT NOT NULL,
                args_json TEXT NOT NULL DEFAULT '{}',
                started_at INTEGER NOT NULL,
                ended_at INTEGER NOT NULL DEFAULT 0,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                exit_code INTEGER NOT NULL DEFAULT 0,
                search_to_export_ms INTEGER,
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE UNIQUE INDEX idx_sessions_session_id ON analytics_sessions(session_id);
            CREATE INDEX idx_sessions_started ON analytics_sessions(started_at);
            CREATE INDEX idx_sessions_command ON analytics_sessions(command);",
        )
        .unwrap();
        conn
    }

    fn ts(offset_secs: i64) -> i64 {
        1700000000 + offset_secs
    }

    #[test]
    fn test_record_session_basic() {
        let conn = setup_test_db();
        record_session(
            &conn, "ses_001", "import", r#"{"name":"film"}"#, ts(0), ts(5000), 5000, 0, None,
        )
        .unwrap();

        let (cmd, dur): (String, i64) = conn
            .query_row(
                "SELECT command, duration_ms FROM analytics_sessions WHERE session_id = 'ses_001'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(cmd, "import");
        assert_eq!(dur, 5000);
    }

    #[test]
    fn test_record_session_with_search_to_export() {
        let conn = setup_test_db();
        record_session(
            &conn, "ses_002", "export", "{}", ts(10000), ts(15000), 5000, 0, Some(8000),
        )
        .unwrap();

        let ste: Option<i64> = conn
            .query_row(
                "SELECT search_to_export_ms FROM analytics_sessions WHERE session_id = 'ses_002'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ste, Some(8000));
    }

    #[test]
    fn test_record_session_upsert() {
        let conn = setup_test_db();
        record_session(&conn, "ses_003", "import", "{}", ts(0), ts(1000), 1000, 0, None).unwrap();
        record_session(&conn, "ses_003", "import", "{}", ts(0), ts(3000), 3000, 0, None).unwrap();

        let dur: i64 = conn
            .query_row(
                "SELECT duration_ms FROM analytics_sessions WHERE session_id = 'ses_003'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dur, 3000); // updated
    }

    #[test]
    fn test_get_last_search_started_at() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "import", "{}", ts(0), ts(1000), 1000, 0, None).unwrap();
        record_session(&conn, "s2", "search", "{}", ts(5000), ts(8000), 3000, 0, None).unwrap();
        record_session(&conn, "s3", "search", "{}", ts(10000), ts(12000), 2000, 0, None).unwrap();

        let result = get_last_search_started_at(&conn).unwrap();
        assert_eq!(result, Some(ts(10000))); // most recent
    }

    #[test]
    fn test_get_last_search_started_at_none() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "import", "{}", ts(0), ts(1000), 1000, 0, None).unwrap();

        let result = get_last_search_started_at(&conn).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_last_search_started_at_before_export() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "search", "{}", ts(0), ts(8000), 8000, 0, None).unwrap();
        record_session(&conn, "s2", "export", "{}", ts(20000), ts(25000), 5000, 0, None).unwrap();

        let result = get_last_search_started_at(&conn).unwrap();
        assert_eq!(result, Some(ts(0))); // still the search session, not export
    }

    #[test]
    fn test_get_session_count_all() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "import", "{}", ts(0), ts(1000), 1000, 0, None).unwrap();
        record_session(&conn, "s2", "search", "{}", ts(2000), ts(3000), 1000, 0, None).unwrap();
        record_session(&conn, "s3", "import", "{}", ts(4000), ts(5000), 1000, 0, None).unwrap();

        assert_eq!(get_session_count(&conn, None).unwrap(), 3);
    }

    #[test]
    fn test_get_session_count_filtered() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "import", "{}", ts(0), ts(1000), 1000, 0, None).unwrap();
        record_session(&conn, "s2", "search", "{}", ts(2000), ts(3000), 1000, 0, None).unwrap();
        record_session(&conn, "s3", "import", "{}", ts(6000), ts(7000), 1000, 0, None).unwrap();

        // Only sessions starting at or after ts(5000)
        assert_eq!(get_session_count(&conn, Some(ts(5000))).unwrap(), 1);
    }

    #[test]
    fn test_get_average_duration_ms_all() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "import", "{}", ts(0), ts(2000), 2000, 0, None).unwrap();
        record_session(&conn, "s2", "search", "{}", ts(3000), ts(5000), 2000, 0, None).unwrap();
        record_session(&conn, "s3", "import", "{}", ts(6000), ts(11000), 5000, 0, None).unwrap();

        // Average of 2000 + 2000 + 5000 = 3000
        assert_eq!(get_average_duration_ms(&conn, None).unwrap(), 3000);
    }

    #[test]
    fn test_get_average_duration_ms_filtered() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "import", "{}", ts(0), ts(2000), 2000, 0, None).unwrap();
        record_session(&conn, "s2", "search", "{}", ts(3000), ts(5000), 2000, 0, None).unwrap();
        record_session(&conn, "s3", "import", "{}", ts(6000), ts(11000), 5000, 0, None).unwrap();

        // Only sessions starting at or after ts(5000): avg of [5000] = 5000
        assert_eq!(get_average_duration_ms(&conn, Some(ts(5000))).unwrap(), 5000);
    }

    #[test]
    fn test_get_average_duration_ms_empty() {
        let conn = setup_test_db();
        // No sessions with exit_code = 0
        assert_eq!(get_average_duration_ms(&conn, None).unwrap(), 0);
    }

    #[test]
    fn test_get_command_breakdown_all() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "import", "{}", ts(0), ts(1000), 1000, 0, None).unwrap();
        record_session(&conn, "s2", "import", "{}", ts(2000), ts(3000), 1000, 0, None).unwrap();
        record_session(&conn, "s3", "search", "{}", ts(4000), ts(5000), 1000, 0, None).unwrap();
        record_session(&conn, "s4", "tag", "{}", ts(6000), ts(7000), 1000, 0, None).unwrap();

        let breakdown = get_command_breakdown(&conn, None).unwrap();
        assert_eq!(breakdown.len(), 3); // import(2), search(1), tag(1)
        assert_eq!(breakdown[0], ("import".to_string(), 2));
        assert_eq!(breakdown[1], ("search".to_string(), 1));
        assert_eq!(breakdown[2], ("tag".to_string(), 1));
    }

    #[test]
    fn test_get_command_breakdown_filtered() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "import", "{}", ts(0), ts(1000), 1000, 0, None).unwrap();
        record_session(&conn, "s2", "search", "{}", ts(2000), ts(3000), 1000, 0, None).unwrap();
        record_session(&conn, "s3", "import", "{}", ts(6000), ts(7000), 1000, 0, None).unwrap();

        let breakdown = get_command_breakdown(&conn, Some(ts(5000))).unwrap();
        assert_eq!(breakdown.len(), 1);
        assert_eq!(breakdown[0], ("import".to_string(), 1));
    }

    #[test]
    fn test_get_sessions_all() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "import", "{}", ts(0), ts(1000), 1000, 0, None).unwrap();
        record_session(&conn, "s2", "search", "{}", ts(2000), ts(3000), 1000, 0, None).unwrap();

        let sessions = get_sessions(&conn, None, 10).unwrap();
        assert_eq!(sessions.len(), 2);
        // Most recent first
        assert_eq!(sessions[0].session_id, "s2");
        assert_eq!(sessions[1].session_id, "s1");
        assert_eq!(sessions[0].command, "search");
    }

    #[test]
    fn test_get_sessions_with_limit() {
        let conn = setup_test_db();
        for i in 0..5 {
            record_session(
                &conn, &format!("s{}", i), "import", "{}", ts(i * 1000), ts(i * 1000 + 500), 500, 0, None,
            )
            .unwrap();
        }

        let sessions = get_sessions(&conn, None, 3).unwrap();
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].session_id, "s4"); // most recent
    }

    #[test]
    fn test_get_sessions_with_filter() {
        let conn = setup_test_db();
        record_session(&conn, "s1", "import", "{}", ts(0), ts(1000), 1000, 0, None).unwrap();
        record_session(&conn, "s2", "search", "{}", ts(5000), ts(6000), 1000, 0, None).unwrap();
        record_session(&conn, "s3", "import", "{}", ts(10000), ts(11000), 1000, 0, None).unwrap();

        let sessions = get_sessions(&conn, Some(ts(6000)), 10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s3");
    }

    #[test]
    fn test_error_display() {
        let err = AnalyticsError::DatabaseError("query failed".to_string());
        assert!(format!("{}", err).contains("ANALYTICS_DB_ERROR"));
    }
}
