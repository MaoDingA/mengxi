// tag.rs — Tag CRUD operations for fingerprints

use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from tag operations.
#[derive(Debug)]
pub enum TagError {
    /// Tag not found on the specified fingerprint.
    NotFound(String),
    /// A database error occurred.
    DatabaseError(String),
    /// Duplicate tag on the same fingerprint.
    DuplicateTag(String),
}

impl std::fmt::Display for TagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagError::NotFound(msg) => {
                write!(f, "TAG_NOT_FOUND -- {}", msg)
            }
            TagError::DatabaseError(msg) => {
                write!(f, "TAG_DB_ERROR -- {}", msg)
            }
            TagError::DuplicateTag(msg) => {
                write!(f, "TAG_DUPLICATE -- {}", msg)
            }
        }
    }
}

impl std::error::Error for TagError {}

// ---------------------------------------------------------------------------
// Tag CRUD functions
// ---------------------------------------------------------------------------

/// Add a tag to a fingerprint.
/// Returns `DuplicateTag` if the tag already exists on this fingerprint.
/// Returns `DatabaseError` if the tag is empty or whitespace-only.
pub fn tag_add(conn: &Connection, fingerprint_id: i64, tag: &str) -> Result<(), TagError> {
    if tag.trim().is_empty() {
        return Err(TagError::DatabaseError(
            "Tag must not be empty or whitespace-only".to_string(),
        ));
    }

    conn.execute(
        "INSERT INTO tags (fingerprint_id, tag) VALUES (?1, ?2)",
        rusqlite::params![fingerprint_id, tag],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            TagError::DuplicateTag(format!(
                "Tag '{}' already exists on fingerprint {}",
                tag, fingerprint_id
            ))
        } else {
            TagError::DatabaseError(e.to_string())
        }
    })?;
    Ok(())
}

/// Remove a tag from a fingerprint.
/// Returns `NotFound` if the tag does not exist on this fingerprint.
pub fn tag_remove(conn: &Connection, fingerprint_id: i64, tag: &str) -> Result<(), TagError> {
    let affected = conn
        .execute(
            "DELETE FROM tags WHERE fingerprint_id = ?1 AND tag = ?2",
            rusqlite::params![fingerprint_id, tag],
        )
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    if affected == 0 {
        return Err(TagError::NotFound(format!(
            "Tag '{}' not found on fingerprint {}",
            tag, fingerprint_id
        )));
    }
    Ok(())
}

/// List all tags for a fingerprint.
pub fn tag_list(conn: &Connection, fingerprint_id: i64) -> Result<Vec<String>, TagError> {
    let mut stmt = conn
        .prepare("SELECT tag FROM tags WHERE fingerprint_id = ?1 ORDER BY tag")
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    let tags: Vec<String> = stmt
        .query_map([fingerprint_id], |row| row.get::<_, String>(0))
        .map_err(|e| TagError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    Ok(tags)
}

/// Get all fingerprint IDs for a given project name.
pub fn fingerprint_ids_for_project(conn: &Connection, project_name: &str) -> Result<Vec<i64>, TagError> {
    let mut stmt = conn
        .prepare(
            "SELECT fp.id FROM fingerprints fp
             JOIN files f ON f.id = fp.file_id
             JOIN projects p ON p.id = f.project_id
             WHERE p.name = ?1",
        )
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    let ids: Vec<i64> = stmt
        .query_map([project_name], |row| row.get::<_, i64>(0))
        .map_err(|e| TagError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    Ok(ids)
}

/// Add a tag to all fingerprints in a project.
pub fn tag_add_to_project(conn: &Connection, project_name: &str, tag: &str) -> Result<usize, TagError> {
    let ids = fingerprint_ids_for_project(conn, project_name)?;
    let mut added = 0;
    let tx = conn.unchecked_transaction().map_err(|e| TagError::DatabaseError(e.to_string()))?;
    for id in &ids {
        match tag_add(&tx, *id, tag) {
            Ok(()) => added += 1,
            Err(TagError::DuplicateTag(_)) => {} // skip duplicates
            Err(e) => return Err(e),
        }
    }
    tx.commit().map_err(|e| TagError::DatabaseError(e.to_string()))?;
    Ok(added)
}

/// Remove a tag from all fingerprints in a project.
pub fn tag_remove_from_project(conn: &Connection, project_name: &str, tag: &str) -> Result<usize, TagError> {
    let ids = fingerprint_ids_for_project(conn, project_name)?;
    let mut removed = 0;
    let tx = conn.unchecked_transaction().map_err(|e| TagError::DatabaseError(e.to_string()))?;
    for id in &ids {
        match tag_remove(&tx, *id, tag) {
            Ok(()) => removed += 1,
            Err(TagError::NotFound(_)) => {} // skip if not present
            Err(e) => return Err(e),
        }
    }
    tx.commit().map_err(|e| TagError::DatabaseError(e.to_string()))?;
    Ok(removed)
}

/// List all unique tags for a project's fingerprints.
pub fn tag_list_for_project(conn: &Connection, project_name: &str) -> Result<Vec<String>, TagError> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT t.tag FROM tags t
             JOIN fingerprints fp ON fp.id = t.fingerprint_id
             JOIN files f ON f.id = fp.file_id
             JOIN projects p ON p.id = f.project_id
             WHERE p.name = ?1
             ORDER BY t.tag",
        )
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    let tags: Vec<String> = stmt
        .query_map([project_name], |row| row.get::<_, String>(0))
        .map_err(|e| TagError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    Ok(tags)
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
             CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, fingerprint_id INTEGER NOT NULL REFERENCES fingerprints(id) ON DELETE CASCADE, tag TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE UNIQUE INDEX idx_tags_fingerprint_tag ON tags(fingerprint_id, tag);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_tag_add_and_list() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add(&conn, 1, "warm").unwrap();
        tag_add(&conn, 1, "industrial").unwrap();

        let tags = tag_list(&conn, 1).unwrap();
        assert_eq!(tags, vec!["industrial", "warm"]); // sorted alphabetically
    }

    #[test]
    fn test_tag_duplicate_rejected() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add(&conn, 1, "warm").unwrap();
        let result = tag_add(&conn, 1, "warm");
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("TAG_DUPLICATE"));
    }

    #[test]
    fn test_tag_remove() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add(&conn, 1, "warm").unwrap();
        tag_remove(&conn, 1, "warm").unwrap();
        assert!(tag_list(&conn, 1).unwrap().is_empty());
    }

    #[test]
    fn test_tag_remove_not_found() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        let result = tag_remove(&conn, 1, "nonexistent");
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("TAG_NOT_FOUND"));
    }

    #[test]
    fn test_tag_add_to_project() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's2.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, '', '', '', 0.3, 0.2, 'acescg')", [])
            .unwrap();

        let added = tag_add_to_project(&conn, "film", "warm").unwrap();
        assert_eq!(added, 2);

        assert_eq!(tag_list(&conn, 1).unwrap(), vec!["warm"]);
        assert_eq!(tag_list(&conn, 2).unwrap(), vec!["warm"]);
    }

    #[test]
    fn test_tag_list_for_project() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add(&conn, 1, "warm").unwrap();
        tag_add(&conn, 1, "industrial").unwrap();

        let tags = tag_list_for_project(&conn, "film").unwrap();
        assert_eq!(tags, vec!["industrial", "warm"]);
    }

    #[test]
    fn test_tag_remove_from_project() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add(&conn, 1, "warm").unwrap();
        let removed = tag_remove_from_project(&conn, "film", "warm").unwrap();
        assert_eq!(removed, 1);
        assert!(tag_list(&conn, 1).unwrap().is_empty());
    }

    #[test]
    fn test_tag_error_display() {
        let err = TagError::NotFound("tag X not found".to_string());
        assert!(format!("{}", err).contains("TAG_NOT_FOUND"));

        let err = TagError::DatabaseError("query failed".to_string());
        assert!(format!("{}", err).contains("TAG_DB_ERROR"));

        let err = TagError::DuplicateTag("duplicate tag".to_string());
        assert!(format!("{}", err).contains("TAG_DUPLICATE"));
    }

    #[test]
    fn test_tag_add_empty_rejected() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        let result = tag_add(&conn, 1, "");
        assert!(result.is_err());
        let result = tag_add(&conn, 1, "   ");
        assert!(result.is_err());
    }
}
