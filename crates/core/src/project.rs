use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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
    pub created_at: i64,
}

/// Error types for import operations.
#[derive(Debug)]
pub enum ImportError {
    PathNotFound(String),
    DuplicateName(String),
    DbError(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::PathNotFound(path) => {
                write!(f, "IMPORT_PATH_NOT_FOUND — Path {} does not exist", path)
            }
            ImportError::DuplicateName(name) => {
                write!(f, "IMPORT_DUPLICATE_NAME — Project '{}' already exists", name)
            }
            ImportError::DbError(msg) => {
                write!(f, "DB_ERROR — {}", msg)
            }
        }
    }
}

impl std::error::Error for ImportError {}

/// Supported file extensions and their format labels.
const SUPPORTED_EXTENSIONS: &[(&str, &str)] = &[
    ("dpx", "dpx"),
    ("exr", "exr"),
    ("mov", "mov"),
    ("DPX", "dpx"),
    ("EXR", "exr"),
    ("MOV", "mov"),
];

/// Scan a directory for supported media files, returning counts per format.
/// Only lists files at the top level of the directory (non-recursive for MVP).
pub fn scan_project_files(
    path: &Path,
) -> Result<Vec<(String, String)>, ImportError> {
    if !path.exists() {
        return Err(ImportError::PathNotFound(path.to_string_lossy().to_string()));
    }

    let mut files = Vec::new();
    let entries = fs::read_dir(path).map_err(|e| ImportError::DbError(e.to_string()))?;

    for entry in entries {
        let entry = entry.map_err(|e| ImportError::DbError(e.to_string()))?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            for (supported_ext, format_label) in SUPPORTED_EXTENSIONS {
                if ext.eq_ignore_ascii_case(supported_ext) {
                    files.push((
                        path.file_name().unwrap().to_string_lossy().to_string(),
                        format_label.to_string(),
                    ));
                    break;
                }
            }
        }
    }

    Ok(files)
}

/// Register a project: scan files, check for duplicates, insert into DB.
pub fn register_project(
    conn: &Connection,
    name: &str,
    path: &Path,
) -> Result<Project, ImportError> {
    // Check for duplicate name
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE name = ?1)",
            [name],
            |row| row.get(0),
        )
        .map_err(|e| ImportError::DbError(e.to_string()))?;

    if exists {
        return Err(ImportError::DuplicateName(name.to_string()));
    }

    // Scan files
    let files = scan_project_files(path)?;
    let dpx_count = files.iter().filter(|(_, f)| f == "dpx").count() as i64;
    let exr_count = files.iter().filter(|(_, f)| f == "exr").count() as i64;
    let mov_count = files.iter().filter(|(_, f)| f == "mov").count() as i64;

    // Insert project record
    conn.execute(
        "INSERT INTO projects (name, path, dpx_count, exr_count, mov_count) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, path.to_string_lossy(), dpx_count, exr_count, mov_count],
    )
    .map_err(|e| ImportError::DbError(e.to_string()))?;

    let project_id = conn.last_insert_rowid();

    // Insert file records
    for (filename, format) in &files {
        conn.execute(
            "INSERT INTO files (project_id, filename, format) VALUES (?1, ?2, ?3)",
            params![project_id, filename, format],
        )
        .map_err(|e| ImportError::DbError(e.to_string()))?;
    }

    Ok(Project {
        id: project_id,
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        dpx_count,
        exr_count,
        mov_count,
        created_at: 0, // Will be populated by query if needed
    })
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
    use tempfile::TempDir;

    fn setup_test_db() -> (TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("test.db");
        let conn = Connection::open(&db_file).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL UNIQUE,
                path        TEXT NOT NULL,
                dpx_count   INTEGER NOT NULL DEFAULT 0,
                exr_count   INTEGER NOT NULL DEFAULT 0,
                mov_count   INTEGER NOT NULL DEFAULT 0,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE TABLE files (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                filename    TEXT NOT NULL,
                format      TEXT NOT NULL CHECK(format IN ('dpx', 'exr', 'mov')),
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .unwrap();
        (dir, conn)
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
    fn test_register_project_success() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film_project");
        create_test_files(&film_dir, &["shot001.dpx", "shot002.dpx", "ref.exr"]);

        let (_db_dir, conn) = setup_test_db();
        let project = register_project(&conn, "my_film", &film_dir).unwrap();

        assert_eq!(project.name, "my_film");
        assert_eq!(project.dpx_count, 2);
        assert_eq!(project.exr_count, 1);
        assert_eq!(project.mov_count, 0);
        assert!(project.id > 0);
    }

    #[test]
    fn test_duplicate_project_name_error() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film_project");
        create_test_files(&film_dir, &["shot.dpx"]);

        let (_db_dir, conn) = setup_test_db();
        register_project(&conn, "my_film", &film_dir).unwrap();
        let result = register_project(&conn, "my_film", &film_dir);

        assert!(result.is_err());
        match result.unwrap_err() {
            ImportError::DuplicateName(name) => assert_eq!(name, "my_film"),
            other => panic!("Expected DuplicateName, got: {:?}", other),
        }
    }

    #[test]
    fn test_nonexistent_path_error() {
        let (_db_dir, conn) = setup_test_db();
        let result = register_project(&conn, "test", Path::new("/nonexistent/path"));

        assert!(result.is_err());
        match result.unwrap_err() {
            ImportError::PathNotFound(path) => {
                assert!(path.contains("/nonexistent"))
            }
            other => panic!("Expected PathNotFound, got: {:?}", other),
        }
    }

    #[test]
    fn test_scan_files_counts_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        create_test_files(
            &film_dir,
            &[
                "a.dpx", "b.dpx", "c.DPX", "d.exr", "e.EXR", "f.mov", "g.MOV", "h.txt",
            ],
        );

        let files = scan_project_files(&film_dir).unwrap();
        assert_eq!(files.len(), 7); // Excludes .txt
        assert_eq!(files.iter().filter(|(_, f)| f == "dpx").count(), 3);
        assert_eq!(files.iter().filter(|(_, f)| f == "exr").count(), 2);
        assert_eq!(files.iter().filter(|(_, f)| f == "mov").count(), 2);
    }

    #[test]
    fn test_list_projects() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        create_test_files(&film_dir, &["shot.dpx"]);

        let (_db_dir, conn) = setup_test_db();
        register_project(&conn, "film_a", &film_dir).unwrap();

        let film_dir2 = dir.path().join("film2");
        create_test_files(&film_dir2, &["ref.exr"]);
        register_project(&conn, "film_b", &film_dir2).unwrap();

        let projects = list_projects(&conn).unwrap();
        assert_eq!(projects.len(), 2);
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"film_a"));
        assert!(names.contains(&"film_b"));
    }
}
