// project.rs — Project data models and DB queries (I/O orchestration moved to CLI/project_ops.rs)

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A registered project in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub dpx_count: i64,
    pub exr_count: i64,
    pub mov_count: i64,
    pub created_at: i64,
}

/// A file belonging to a registered project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFile {
    pub id: i64,
    pub project_id: i64,
    pub filename: String,
    pub format: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub bit_depth: Option<i64>,
    pub transfer: Option<String>,
    pub colorimetric: Option<String>,
    pub descriptor: Option<String>,
    pub compression: Option<String>,
    pub codec: Option<String>,
    pub fps: Option<f64>,
    pub duration: Option<f64>,
    pub frame_count: Option<i64>,
    pub created_at: i64,
}

/// Per-variant breakdown for import summary.
#[derive(Debug, Clone, Default)]
pub struct VariantBreakdown {
    pub variants: Vec<String>, // e.g., "5x 10-bit linear", "3x 16-bit linear"
    pub skipped_count: usize,
    pub skipped_files: Vec<String>,
    pub fingerprint_count: usize,
    pub grading_feature_count: usize,
    pub resumed_count: usize,
}

/// Error types for import operations.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("IMPORT_PATH_NOT_FOUND — Path {0} does not exist")]
    PathNotFound(String),
    #[error("IMPORT_DUPLICATE_NAME — Project '{0}' already exists")]
    DuplicateName(String),
    #[error("DB_ERROR — {0}")]
    DbError(String),
    #[error("IMPORT_CORRUPT_FILE -- Failed to decode {filename}: {reason}")]
    CorruptFile { filename: String, reason: String },
}

/// Map DPX transfer characteristic string to a color space tag for fingerprint extraction.
/// Delegates to the shared feature_pipeline module.
#[cfg(test)]
pub(crate) fn map_transfer_string_to_color_tag(transfer: &str) -> String {
    crate::feature_pipeline::map_transfer_string_to_color_tag(transfer)
}

/// Retrieve a project by name.
pub fn get_project(conn: &Connection, name: &str) -> Result<Option<Project>, ImportError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, path, dpx_count, exr_count, mov_count, created_at FROM projects WHERE name = ?1",
        )
        .map_err(|e| ImportError::DbError(e.to_string()))?;

    let project = stmt
        .query_row([name], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                dpx_count: row.get(3)?,
                exr_count: row.get(4)?,
                mov_count: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .ok();

    Ok(project)
}

/// Retrieve all projects from the database.
pub fn list_projects(conn: &Connection) -> Result<Vec<Project>, ImportError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, path, dpx_count, exr_count, mov_count, created_at FROM projects ORDER BY created_at DESC",
        )
        .map_err(|e| ImportError::DbError(e.to_string()))?;

    let projects = stmt
        .query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                dpx_count: row.get(3)?,
                exr_count: row.get(4)?,
                mov_count: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| ImportError::DbError(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ImportError::DbError(e.to_string()))?;

    Ok(projects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn setup_test_db() -> (TempDir, Connection) {
        crate::test_db::setup_test_db_file()
    }

    fn create_test_files(dir: &Path, files: &[&str]) {
        for f in files {
            let path = dir.join(f);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, "").unwrap();
        }
    }

    #[test]
    fn test_list_projects() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        create_test_files(&film_dir, &["shot.dpx"]);

        let (_db_dir, conn) = setup_test_db();
        // Insert project directly (register_project is now in CLI)
        conn.execute("INSERT INTO projects (name, path, dpx_count, exr_count, mov_count) VALUES ('film_a', '/tmp/film', 1, 0, 0)", []).unwrap();

        let film_dir2 = dir.path().join("film2");
        create_test_files(&film_dir2, &["ref.exr"]);
        conn.execute("INSERT INTO projects (name, path, dpx_count, exr_count, mov_count) VALUES ('film_b', '/tmp/film2', 0, 1, 0)", []).unwrap();

        let projects = list_projects(&conn).unwrap();
        assert_eq!(projects.len(), 2);
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"film_a"));
        assert!(names.contains(&"film_b"));
    }

    #[test]
    fn test_get_project_found() {
        let (_db_dir, conn) = setup_test_db();
        conn.execute("INSERT INTO projects (name, path, dpx_count, exr_count, mov_count) VALUES ('test', '/tmp/test', 1, 0, 0)", []).unwrap();

        let project = get_project(&conn, "test").unwrap();
        assert!(project.is_some());
        assert_eq!(project.unwrap().name, "test");
    }

    #[test]
    fn test_get_project_not_found() {
        let (_db_dir, conn) = setup_test_db();
        let project = get_project(&conn, "nonexistent").unwrap();
        assert!(project.is_none());
    }

    #[test]
    fn test_import_error_display() {
        let err = ImportError::PathNotFound("/bad".to_string());
        assert!(err.to_string().contains("IMPORT_PATH_NOT_FOUND"));

        let err = ImportError::DuplicateName("dup".to_string());
        assert!(err.to_string().contains("IMPORT_DUPLICATE_NAME"));

        let err = ImportError::DbError("fail".to_string());
        assert!(err.to_string().contains("DB_ERROR"));

        let err = ImportError::CorruptFile { filename: "x".to_string(), reason: "bad".to_string() };
        assert!(err.to_string().contains("IMPORT_CORRUPT_FILE"));
    }

    #[test]
    fn test_transfer_string_mapping() {
        assert_eq!(map_transfer_string_to_color_tag("printing_density"), "log");
        assert_eq!(map_transfer_string_to_color_tag("logarithmic"), "log");
        assert_eq!(map_transfer_string_to_color_tag("bt709"), "video");
        assert_eq!(map_transfer_string_to_color_tag("bt601_bg"), "video");
        assert_eq!(map_transfer_string_to_color_tag("smpte_274m"), "video");
        assert_eq!(map_transfer_string_to_color_tag("linear"), "linear");
        assert_eq!(map_transfer_string_to_color_tag("user_defined"), "linear");
    }
}
