// calibration.rs — Tag calibration learning loop

use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from calibration operations.
#[derive(Debug, thiserror::Error)]
pub enum CalibrationError {
    /// A database error occurred.
    #[error("CALIBRATION_DB_ERROR -- {0}")]
    DatabaseError(String),
}

// ---------------------------------------------------------------------------
// Calibration record functions
// ---------------------------------------------------------------------------

/// A single calibration record.
#[derive(Debug, Clone)]
pub struct CalibrationRecord {
    pub id: i64,
    pub project_name: String,
    pub fingerprint_id: i64,
    pub removed_tags: String,
    pub added_tags: String,
    pub renamed_tags: String,
    pub created_at: i64,
}

/// Record a calibration event when a colorist corrects AI-generated tags.
pub fn record_calibration(
    conn: &Connection,
    project_name: &str,
    fingerprint_id: i64,
    removed_tags: &str,
    added_tags: &str,
    renamed_tags: &str,
) -> Result<(), CalibrationError> {
    conn.execute(
        "INSERT INTO calibration_activities (project_name, fingerprint_id, removed_tags, added_tags, renamed_tags)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![project_name, fingerprint_id, removed_tags, added_tags, renamed_tags],
    )
    .map_err(|e| CalibrationError::DatabaseError(e.to_string()))?;
    Ok(())
}

/// Get all calibration history records.
pub fn get_calibration_history(conn: &Connection) -> Result<Vec<CalibrationRecord>, CalibrationError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_name, fingerprint_id, removed_tags, added_tags, renamed_tags, created_at
             FROM calibration_activities
             ORDER BY created_at DESC",
        )
        .map_err(|e| CalibrationError::DatabaseError(e.to_string()))?;

    let records: Vec<CalibrationRecord> = stmt
        .query_map([], |row| {
            Ok(CalibrationRecord {
                id: row.get(0)?,
                project_name: row.get(1)?,
                fingerprint_id: row.get(2)?,
                removed_tags: row.get(3)?,
                added_tags: row.get(4)?,
                renamed_tags: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| CalibrationError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| CalibrationError::DatabaseError(e.to_string()))?;

    Ok(records)
}

/// Get personalized tags built from all manual tags across the system.
/// Merges unique `source = "manual"` tags from the tags table with all
/// `added_tags` from calibration_activities. Returns tags ordered by frequency
/// (most-used first).
pub fn get_personalized_tags(conn: &Connection) -> Result<Vec<String>, CalibrationError> {
    let mut stmt = conn
        .prepare(
            "SELECT tag, SUM(cnt) as total FROM (
                SELECT tag, COUNT(*) as cnt FROM tags WHERE source = 'manual' GROUP BY tag
                UNION ALL
                SELECT value, 1 as cnt FROM calibration_activities, json_each(added_tags)
            ) GROUP BY tag
            ORDER BY total DESC",
        )
        .map_err(|e| CalibrationError::DatabaseError(e.to_string()))?;

    let tags: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| CalibrationError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| CalibrationError::DatabaseError(e.to_string()))?;

    Ok(tags)
}

/// Get the total count of calibration records.
pub fn get_calibration_count(conn: &Connection) -> Result<usize, CalibrationError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM calibration_activities",
            [],
            |row| row.get(0),
        )
        .map_err(|e| CalibrationError::DatabaseError(e.to_string()))?;
    Ok(count as usize)
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
            "CREATE TABLE projects (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, path TEXT NOT NULL, dpx_count INTEGER NOT NULL DEFAULT 0, exr_count INTEGER NOT NULL DEFAULT 0, mov_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE, filename TEXT NOT NULL, format TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE TABLE fingerprints (id INTEGER PRIMARY KEY AUTOINCREMENT, file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE, histogram_r TEXT NOT NULL, histogram_g TEXT NOT NULL, histogram_b TEXT NOT NULL, luminance_mean REAL NOT NULL, luminance_stddev REAL NOT NULL, color_space_tag TEXT NOT NULL, embedding BLOB, embedding_model TEXT, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, fingerprint_id INTEGER NOT NULL REFERENCES fingerprints(id) ON DELETE CASCADE, tag TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()), source TEXT NOT NULL DEFAULT 'ai');
             CREATE UNIQUE INDEX idx_tags_fingerprint_tag ON tags(fingerprint_id, tag);
             CREATE TABLE calibration_activities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_name TEXT NOT NULL,
                fingerprint_id INTEGER NOT NULL REFERENCES fingerprints(id) ON DELETE CASCADE,
                removed_tags TEXT NOT NULL DEFAULT '[]',
                added_tags TEXT NOT NULL DEFAULT '[]',
                renamed_tags TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );
             CREATE INDEX idx_calibration_project ON calibration_activities(project_name);
             CREATE INDEX idx_calibration_created ON calibration_activities(created_at);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_record_calibration() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        record_calibration(&conn, "film", 1, r#"["cold"]"#, r#"["cool blue shadows"]"#, r#"[]"#).unwrap();

        let count = get_calibration_count(&conn).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_record_calibration_with_rename() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        record_calibration(&conn, "film", 1, r#"[]"#, r#"[]"#, r#"[{"old":"cold","new":"cool blue shadows"}]"#).unwrap();

        let history = get_calibration_history(&conn).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].renamed_tags, r#"[{"old":"cold","new":"cool blue shadows"}]"#);
    }

    #[test]
    fn test_get_calibration_history() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        record_calibration(&conn, "film", 1, r#"["cold"]"#, r#"["cool blue"]"#, r#"[]"#).unwrap();
        record_calibration(&conn, "film", 1, r#"[]"#, r#"["SK-II skin"]"#, r#"[]"#).unwrap();

        let history = get_calibration_history(&conn).unwrap();
        assert_eq!(history.len(), 2);
        // Most recent first
        assert_eq!(history[0].added_tags, r#"["SK-II skin"]"#);
        assert_eq!(history[1].added_tags, r#"["cool blue"]"#);
    }

    #[test]
    fn test_get_calibration_count_empty() {
        let conn = setup_test_db();
        let count = get_calibration_count(&conn).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_get_personalized_tags_from_manual_tags() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        // Add manual tags
        conn.execute("INSERT INTO tags (fingerprint_id, tag, source) VALUES (1, 'SK-II skin', 'manual')", [])
            .unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag, source) VALUES (1, 'ethereal warm', 'manual')", [])
            .unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag, source) VALUES (1, 'industrial', 'ai')", [])
            .unwrap();

        let tags = get_personalized_tags(&conn).unwrap();
        // Only manual tags should appear, not "industrial" (AI)
        assert!(tags.contains(&"SK-II skin".to_string()));
        assert!(tags.contains(&"ethereal warm".to_string()));
        assert!(!tags.contains(&"industrial".to_string()));
    }

    #[test]
    fn test_get_personalized_tags_from_calibration() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        // Record calibration with added tags
        record_calibration(&conn, "film", 1, r#"[]"#, r#"["cool blue shadows"]"#, r#"[]"#).unwrap();
        record_calibration(&conn, "film", 1, r#"[]"#, r#"["cool blue shadows"]"#, r#"[]"#).unwrap();
        record_calibration(&conn, "film", 1, r#"[]"#, r#"["SK-II skin"]"#, r#"[]"#).unwrap();

        let tags = get_personalized_tags(&conn).unwrap();
        // "cool blue shadows" appears twice, "SK-II skin" once — should be ordered by frequency
        assert_eq!(tags[0], "cool blue shadows");
        assert!(tags.contains(&"SK-II skin".to_string()));
    }

    #[test]
    fn test_get_personalized_tags_empty() {
        let conn = setup_test_db();
        let tags = get_personalized_tags(&conn).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn test_get_personalized_tags_dedup_across_sources() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        // Add manual tag
        conn.execute("INSERT INTO tags (fingerprint_id, tag, source) VALUES (1, 'cool blue shadows', 'manual')", [])
            .unwrap();
        // Record calibration with same tag
        record_calibration(&conn, "film", 1, r#"[]"#, r#"["cool blue shadows"]"#, r#"[]"#).unwrap();
        // Add another tag only in calibration
        record_calibration(&conn, "film", 1, r#"[]"#, r#"["SK-II skin"]"#, r#"[]"#).unwrap();

        let tags = get_personalized_tags(&conn).unwrap();
        // "cool blue shadows" appears in both sources (count 1 + 1 = 2), "SK-II skin" only in calibration (count 1)
        assert_eq!(tags.len(), 2); // deduplicated
        assert_eq!(tags[0], "cool blue shadows"); // higher frequency
        assert_eq!(tags[1], "SK-II skin");
    }

    #[test]
    fn test_calibration_error_display() {
        let err = CalibrationError::DatabaseError("query failed".to_string());
        assert!(format!("{}", err).contains("CALIBRATION_DB_ERROR"));
    }
}
