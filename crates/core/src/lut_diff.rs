// lut_diff.rs — LUT diff comparison and dependency tracking
// Bridges CLI ↔ Format (diff) and CLI ↔ DB (dependencies)

use rusqlite::Connection;
use std::path::{Path, PathBuf};

use mengxi_format::lut::{self, LutError};

// ---------------------------------------------------------------------------
// Diff orchestration
// ---------------------------------------------------------------------------

/// Errors from LUT diff operations.
#[derive(Debug)]
pub enum LutDiffError {
    /// One of the LUT files could not be read.
    FileNotFound(PathBuf),
    /// Failed to parse a LUT file.
    ParseError(LutError),
    /// Grid sizes differ between the two LUTs.
    GridSizeMismatch { a: u32, b: u32 },
}

impl std::fmt::Display for LutDiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LutDiffError::FileNotFound(p) => {
                write!(f, "LUTDIFF_IO_ERROR -- file not found: {}", p.display())
            }
            LutDiffError::ParseError(e) => {
                write!(f, "LUTDIFF_PARSE_ERROR -- {}", e)
            }
            LutDiffError::GridSizeMismatch { a, b } => {
                write!(
                    f,
                    "LUTDIFF_GRID_MISMATCH -- grid sizes differ: {} vs {}",
                    a, b
                )
            }
        }
    }
}

impl std::error::Error for LutDiffError {}

impl From<LutError> for LutDiffError {
    fn from(e: LutError) -> Self {
        LutDiffError::ParseError(e)
    }
}

/// Compare two LUT files and return the diff result.
///
/// Both files are parsed to `LutData` before comparison, so any supported format
/// can be compared against any other supported format.
pub fn compare_luts(path_a: &Path, path_b: &Path) -> Result<lut::LutDiffResult, LutDiffError> {
    if !path_a.exists() {
        return Err(LutDiffError::FileNotFound(path_a.to_path_buf()));
    }
    if !path_b.exists() {
        return Err(LutDiffError::FileNotFound(path_b.to_path_buf()));
    }

    let a = lut::parse_lut(path_a).map_err(LutDiffError::ParseError)?;
    let b = lut::parse_lut(path_b).map_err(LutDiffError::ParseError)?;

    a.diff(&b).map_err(|e| match e {
        LutError::GridSizeMismatch { a, b } => LutDiffError::GridSizeMismatch { a, b },
        other => LutDiffError::ParseError(other),
    })
}

// ---------------------------------------------------------------------------
// Dependency tracking
// ---------------------------------------------------------------------------

/// A LUT dependency record from the database.
#[derive(Debug, Clone)]
pub struct LutDependency {
    pub project_name: String,
    pub file_path: String,
    pub format: String,
    pub grid_size: i64,
    pub exported_at: i64,
}

/// Errors from LUT dependency queries.
#[derive(Debug)]
pub enum LutDepError {
    /// Missing required argument.
    MissingArg(String),
    /// Database initialization failed.
    DbError(String),
    /// Database query error.
    QueryError(String),
}

impl std::fmt::Display for LutDepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LutDepError::MissingArg(msg) => {
                write!(f, "LUTDEP_MISSING_ARG -- {}", msg)
            }
            LutDepError::DbError(msg) => {
                write!(f, "LUTDEP_DB_ERROR -- {}", msg)
            }
            LutDepError::QueryError(msg) => {
                write!(f, "LUTDEP_DB_ERROR -- {}", msg)
            }
        }
    }
}

impl std::error::Error for LutDepError {}

/// Query the database for dependency records matching a LUT file path.
///
/// Matches against the `output_path` column in the `luts` table.
/// Returns `Ok(None)` if no records are found.
pub fn query_lut_dependency(
    conn: &Connection,
    lut_path: &str,
) -> Result<Option<LutDependency>, LutDepError> {
    let sql = "
        SELECT p.name, COALESCE(f.filename, ''), l.format, l.grid_size, l.created_at
        FROM luts l
        JOIN projects p ON p.id = l.project_id
        LEFT JOIN files f ON f.id = l.fingerprint_id
        WHERE l.output_path = ?1
        ORDER BY l.created_at DESC
    ";

    let result = conn.query_row(sql, [lut_path], |row| {
        Ok(LutDependency {
            project_name: row.get(0)?,
            file_path: row.get(1)?,
            format: row.get(2)?,
            grid_size: row.get(3)?,
            exported_at: row.get(4)?,
        })
    });

    match result {
        Ok(dep) => Ok(Some(dep)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(LutDepError::QueryError(e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use mengxi_format::lut::LutData;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, path TEXT NOT NULL, dpx_count INTEGER NOT NULL DEFAULT 0, exr_count INTEGER NOT NULL DEFAULT 0, mov_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE, filename TEXT NOT NULL, format TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE TABLE fingerprints (id INTEGER PRIMARY KEY AUTOINCREMENT, file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE, histogram_r TEXT NOT NULL, histogram_g TEXT NOT NULL, histogram_b TEXT NOT NULL, luminance_mean REAL NOT NULL, luminance_stddev REAL NOT NULL, color_space_tag TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE TABLE luts (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE, fingerprint_id INTEGER REFERENCES fingerprints(id), title TEXT, format TEXT NOT NULL CHECK(format IN ('cube', '3dl', 'look', 'csp', 'cdl')), grid_size INTEGER NOT NULL, output_path TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()));",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_compare_luts_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.cube");
        lut::serialize_lut(&LutData::identity(5), &path).unwrap();

        let result = compare_luts(&path, &path).unwrap();
        assert_eq!(result.total_points, 125);
        for ch in 0..3 {
            assert!(result.channels[ch].mean_delta < 1e-15);
            assert_eq!(result.channels[ch].changed_count, 0);
        }
    }

    #[test]
    fn test_compare_luts_file_not_found() {
        let result = compare_luts(
            Path::new("/nonexistent/a.cube"),
            Path::new("/nonexistent/b.cube"),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            LutDiffError::FileNotFound(p) => {
                assert!(p.to_str().unwrap().contains("nonexistent"));
            }
            other => panic!("Expected FileNotFound, got: {:?}", other),
        }
    }

    #[test]
    fn test_query_lut_dependency_found() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test_project', '/tmp/test')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files (project_id, filename, format) VALUES (1, 'scene001.dpx', 'dpx')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '[]', '[]', '[]', 0.5, 0.1, 'acescg')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO luts (project_id, fingerprint_id, title, format, grid_size, output_path) VALUES (1, 1, 'LUT: cube', 'cube', 33, '/home/user/lut/grade.cube')",
            [],
        )
        .unwrap();

        let dep = query_lut_dependency(&conn, "/home/user/lut/grade.cube").unwrap();
        assert!(dep.is_some());
        let d = dep.unwrap();
        assert_eq!(d.project_name, "test_project");
        assert_eq!(d.file_path, "scene001.dpx");
        assert_eq!(d.format, "cube");
        assert_eq!(d.grid_size, 33);
    }

    #[test]
    fn test_query_lut_dependency_not_found() {
        let conn = setup_test_db();
        let dep = query_lut_dependency(&conn, "/nonexistent/lut.cube").unwrap();
        assert!(dep.is_none());
    }

    #[test]
    fn test_query_lut_dependency_no_fingerprint() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test_project', '/tmp/test')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO luts (project_id, fingerprint_id, title, format, grid_size, output_path) VALUES (1, NULL, 'LUT: cube', 'cube', 33, '/home/user/lut/grade.cube')",
            [],
        )
        .unwrap();

        let dep = query_lut_dependency(&conn, "/home/user/lut/grade.cube").unwrap();
        assert!(dep.is_some());
        let d = dep.unwrap();
        assert_eq!(d.project_name, "test_project");
        assert_eq!(d.file_path, ""); // LEFT JOIN returns NULL as empty string
    }

    #[test]
    fn test_error_display() {
        let err = LutDiffError::FileNotFound(PathBuf::from("/tmp/test.cube"));
        assert!(format!("{}", err).contains("LUTDIFF_IO_ERROR"));

        let err = LutDiffError::GridSizeMismatch { a: 17, b: 33 };
        assert!(format!("{}", err).contains("LUTDIFF_GRID_MISMATCH"));

        let err = LutDepError::MissingArg("--lut is required".to_string());
        assert!(format!("{}", err).contains("LUTDEP_MISSING_ARG"));

        let err = LutDepError::DbError("init failed".to_string());
        assert!(format!("{}", err).contains("LUTDEP_DB_ERROR"));
    }
}
