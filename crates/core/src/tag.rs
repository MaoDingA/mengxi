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

/// Add a tag to a fingerprint with a source indicator.
/// Returns `DuplicateTag` if the tag already exists on this fingerprint.
/// Returns `DatabaseError` if the tag is empty or whitespace-only.
pub fn tag_add_with_source(conn: &Connection, fingerprint_id: i64, tag: &str, source: &str) -> Result<(), TagError> {
    if tag.trim().is_empty() {
        return Err(TagError::DatabaseError(
            "Tag must not be empty or whitespace-only".to_string(),
        ));
    }

    conn.execute(
        "INSERT INTO tags (fingerprint_id, tag, source) VALUES (?1, ?2, ?3)",
        rusqlite::params![fingerprint_id, tag, source],
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

/// Add a tag to a fingerprint (default source = "ai").
pub fn tag_add(conn: &Connection, fingerprint_id: i64, tag: &str) -> Result<(), TagError> {
    tag_add_with_source(conn, fingerprint_id, tag, "ai")
}

/// Remove a tag from a fingerprint.
/// Returns `NotFound` if the tag does not exist on this fingerprint.
pub fn tag_remove(conn: &Connection, fingerprint_id: i64, tag: &str) -> Result<(), TagError> {
    if tag.trim().is_empty() {
        return Err(TagError::DatabaseError(
            "Tag must not be empty or whitespace-only".to_string(),
        ));
    }

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

/// Add a tag to all fingerprints in a project with a source indicator.
pub fn tag_add_to_project_with_source(conn: &Connection, project_name: &str, tag: &str, source: &str) -> Result<usize, TagError> {
    let ids = fingerprint_ids_for_project(conn, project_name)?;
    let mut added = 0;
    let tx = conn.unchecked_transaction().map_err(|e| TagError::DatabaseError(e.to_string()))?;
    for id in &ids {
        match tag_add_with_source(&tx, *id, tag, source) {
            Ok(()) => added += 1,
            Err(TagError::DuplicateTag(_)) => {} // skip duplicates
            Err(e) => return Err(e),
        }
    }
    tx.commit().map_err(|e| TagError::DatabaseError(e.to_string()))?;
    Ok(added)
}

/// Add a tag to all fingerprints in a project (default source = "manual").
pub fn tag_add_to_project(conn: &Connection, project_name: &str, tag: &str) -> Result<usize, TagError> {
    tag_add_to_project_with_source(conn, project_name, tag, "manual")
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

/// List all tags for a fingerprint with their source indicator.
/// Returns (tag, source) pairs.
pub fn tag_list_with_source(conn: &Connection, fingerprint_id: i64) -> Result<Vec<(String, String)>, TagError> {
    let mut stmt = conn
        .prepare("SELECT tag, source FROM tags WHERE fingerprint_id = ?1 ORDER BY tag")
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    let tags: Vec<(String, String)> = stmt
        .query_map([fingerprint_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| TagError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    Ok(tags)
}

/// List all unique tags for a project with their source indicator.
/// Returns (tag, source) pairs. If a tag has multiple sources across fingerprints,
/// sources are merged with ", " separator (e.g., "ai, manual").
pub fn tag_list_for_project_with_source(conn: &Connection, project_name: &str) -> Result<Vec<(String, String)>, TagError> {
    let mut stmt = conn
        .prepare(
            "SELECT t.tag, GROUP_CONCAT(t.source, ', ') FROM tags t
             JOIN fingerprints fp ON fp.id = t.fingerprint_id
             JOIN files f ON f.id = fp.file_id
             JOIN projects p ON p.id = f.project_id
             WHERE p.name = ?1
             GROUP BY t.tag
             ORDER BY t.tag",
        )
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    let tags: Vec<(String, String)> = stmt
        .query_map([project_name], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| TagError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| TagError::DatabaseError(e.to_string()))?;

    Ok(tags)
}

/// Rename a tag for a specific fingerprint.
/// Preserves the `source` column value during rename.
/// Returns `NotFound` if the old tag does not exist.
/// Returns `DuplicateTag` if the new tag already exists for this fingerprint.
pub fn tag_rename(conn: &Connection, fingerprint_id: i64, old_tag: &str, new_tag: &str) -> Result<(), TagError> {
    if old_tag.trim().is_empty() {
        return Err(TagError::DatabaseError(
            "Tag must not be empty or whitespace-only".to_string(),
        ));
    }
    if new_tag.trim().is_empty() {
        return Err(TagError::DatabaseError(
            "New tag must not be empty or whitespace-only".to_string(),
        ));
    }

    let affected = conn
        .execute(
            "UPDATE tags SET tag = ?1 WHERE fingerprint_id = ?2 AND tag = ?3",
            rusqlite::params![new_tag, fingerprint_id, old_tag],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                TagError::DuplicateTag(format!(
                    "Tag '{}' already exists on fingerprint {}",
                    new_tag, fingerprint_id
                ))
            } else {
                TagError::DatabaseError(e.to_string())
            }
        })?;

    if affected == 0 {
        return Err(TagError::NotFound(format!(
            "Tag '{}' not found on fingerprint {}",
            old_tag, fingerprint_id
        )));
    }
    Ok(())
}

/// Rename a tag across all fingerprints in a project.
/// Preserves the `source` column value during rename.
/// Returns the number of fingerprints where the tag was renamed.
/// Returns `NotFound` if the tag was not found on any fingerprint in the project.
pub fn tag_rename_in_project(conn: &Connection, project_name: &str, old_tag: &str, new_tag: &str) -> Result<usize, TagError> {
    if old_tag.trim().is_empty() {
        return Err(TagError::DatabaseError(
            "Tag must not be empty or whitespace-only".to_string(),
        ));
    }
    if new_tag.trim().is_empty() {
        return Err(TagError::DatabaseError(
            "New tag must not be empty or whitespace-only".to_string(),
        ));
    }

    let ids = fingerprint_ids_for_project(conn, project_name)?;
    let mut renamed = 0;
    let tx = conn.unchecked_transaction().map_err(|e| TagError::DatabaseError(e.to_string()))?;
    for id in &ids {
        match tag_rename(&tx, *id, old_tag, new_tag) {
            Ok(()) => renamed += 1,
            Err(TagError::NotFound(_)) => {} // skip if not present on this fingerprint
            Err(e) => {
                let _ = tx.rollback();
                return Err(e);
            }
        }
    }

    if renamed == 0 {
        tx.rollback().map_err(|e| TagError::DatabaseError(e.to_string()))?;
        return Err(TagError::NotFound(format!(
            "Tag '{}' not found in project '{}'",
            old_tag, project_name
        )));
    }

    tx.commit().map_err(|e| TagError::DatabaseError(e.to_string()))?;
    Ok(renamed)
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

    #[test]
    fn test_tag_add_with_source() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add_with_source(&conn, 1, "manual_tag", "manual").unwrap();
        tag_add_with_source(&conn, 1, "ai_tag", "ai").unwrap();

        // Verify source values stored correctly
        let (src1,): (String,) = conn.query_row(
            "SELECT source FROM tags WHERE tag = 'manual_tag' AND fingerprint_id = 1",
            [],
            |row| Ok((row.get(0)?,)),
        ).unwrap();
        assert_eq!(src1, "manual");

        let (src2,): (String,) = conn.query_row(
            "SELECT source FROM tags WHERE tag = 'ai_tag' AND fingerprint_id = 1",
            [],
            |row| Ok((row.get(0)?,)),
        ).unwrap();
        assert_eq!(src2, "ai");
    }

    #[test]
    fn test_tag_add_default_source_is_ai() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add(&conn, 1, "default_source").unwrap();

        let (src,): (String,) = conn.query_row(
            "SELECT source FROM tags WHERE tag = 'default_source' AND fingerprint_id = 1",
            [],
            |row| Ok((row.get(0)?,)),
        ).unwrap();
        assert_eq!(src, "ai");
    }

    #[test]
    fn test_tag_add_to_project_with_source() {
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

        let added = tag_add_to_project_with_source(&conn, "film", "manual_tag", "manual").unwrap();
        assert_eq!(added, 2);

        // Verify both fingerprints have source = "manual"
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tags WHERE tag = 'manual_tag' AND source = 'manual'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_tag_add_to_project_default_source_is_manual() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add_to_project(&conn, "film", "default_project_tag").unwrap();

        let (src,): (String,) = conn.query_row(
            "SELECT source FROM tags WHERE tag = 'default_project_tag' LIMIT 1",
            [],
            |row| Ok((row.get(0)?,)),
        ).unwrap();
        assert_eq!(src, "manual");
    }

    #[test]
    fn test_tag_rename() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add_with_source(&conn, 1, "old_name", "ai").unwrap();
        tag_rename(&conn, 1, "old_name", "new_name").unwrap();

        let tags = tag_list(&conn, 1).unwrap();
        assert_eq!(tags, vec!["new_name"]);

        // Verify source preserved
        let (src,): (String,) = conn.query_row(
            "SELECT source FROM tags WHERE tag = 'new_name'",
            [],
            |row| Ok((row.get(0)?,)),
        ).unwrap();
        assert_eq!(src, "ai");
    }

    #[test]
    fn test_tag_rename_not_found() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        let result = tag_rename(&conn, 1, "nonexistent", "new_name");
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("TAG_NOT_FOUND"));
    }

    #[test]
    fn test_tag_rename_duplicate() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add(&conn, 1, "existing_tag").unwrap();
        tag_add(&conn, 1, "to_rename").unwrap();

        let result = tag_rename(&conn, 1, "to_rename", "existing_tag");
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("TAG_DUPLICATE"));
    }

    #[test]
    fn test_tag_rename_empty_new_tag() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add(&conn, 1, "old").unwrap();

        let result = tag_rename(&conn, 1, "old", "");
        assert!(result.is_err());

        let result = tag_rename(&conn, 1, "old", "   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_tag_rename_in_project() {
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

        tag_add_with_source(&conn, 1, "industrial warm", "ai").unwrap();
        tag_add_with_source(&conn, 2, "industrial warm", "ai").unwrap();

        let renamed = tag_rename_in_project(&conn, "film", "industrial warm", "warm industrial").unwrap();
        assert_eq!(renamed, 2);

        // Verify old tag gone, new tag present
        assert_eq!(tag_list(&conn, 1).unwrap(), vec!["warm industrial"]);
        assert_eq!(tag_list(&conn, 2).unwrap(), vec!["warm industrial"]);

        // Verify source preserved
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tags WHERE tag = 'warm industrial' AND source = 'ai'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_tag_rename_in_project_not_found() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        let result = tag_rename_in_project(&conn, "film", "nonexistent", "new_name");
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("TAG_NOT_FOUND"));
    }

    #[test]
    fn test_tag_list_with_source() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add_with_source(&conn, 1, "warm", "ai").unwrap();
        tag_add_with_source(&conn, 1, "SK-II skin", "manual").unwrap();
        tag_add_with_source(&conn, 1, "cool shadows", "ai").unwrap();

        let tags = tag_list_with_source(&conn, 1).unwrap();
        assert_eq!(tags, vec![
            ("SK-II skin".to_string(), "manual".to_string()),
            ("cool shadows".to_string(), "ai".to_string()),
            ("warm".to_string(), "ai".to_string()),
        ]);
    }

    #[test]
    fn test_tag_list_for_project_with_source() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '', '', '', 0.5, 0.1, 'acescg')", [])
            .unwrap();

        tag_add_with_source(&conn, 1, "industrial", "ai").unwrap();
        tag_add_with_source(&conn, 1, "SK-II skin", "manual").unwrap();

        let tags = tag_list_for_project_with_source(&conn, "film").unwrap();
        assert_eq!(tags, vec![
            ("SK-II skin".to_string(), "manual".to_string()),
            ("industrial".to_string(), "ai".to_string()),
        ]);
    }
}
