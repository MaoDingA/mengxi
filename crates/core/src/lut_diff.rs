// lut_diff.rs — LUT diff comparison and dependency tracking
// Bridges CLI ↔ LutIo trait (diff) and CLI ↔ DB (dependencies)

use crate::format_traits::{LutDiffResult as CoreLutDiffResult, LutIo, LutIoError};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Diff orchestration
// ---------------------------------------------------------------------------

/// Errors from LUT diff operations.
#[derive(Debug, thiserror::Error)]
pub enum LutDiffError {
    /// One of the LUT files could not be read.
    #[error("LUTDIFF_IO_ERROR -- file not found: {}", .0.display())]
    FileNotFound(PathBuf),
    /// Failed to parse a LUT file.
    #[error("LUTDIFF_PARSE_ERROR -- {0}")]
    ParseError(#[from] LutIoError),
    /// Grid sizes differ between the two LUTs.
    #[error("LUTDIFF_GRID_MISMATCH -- grid sizes differ: {a} vs {b}")]
    GridSizeMismatch { a: u32, b: u32 },
}

/// Compare two LUT files and return the diff result.
///
/// Both files are parsed to `LutData` before comparison, so any supported format
/// can be compared against any other supported format.
pub fn compare_luts(
    lut_io: &dyn LutIo,
    path_a: &Path,
    path_b: &Path,
) -> Result<CoreLutDiffResult, LutDiffError> {
    if !path_a.exists() {
        return Err(LutDiffError::FileNotFound(path_a.to_path_buf()));
    }
    if !path_b.exists() {
        return Err(LutDiffError::FileNotFound(path_b.to_path_buf()));
    }

    let a = lut_io.parse_lut(path_a)?;
    let b = lut_io.parse_lut(path_b)?;

    a.diff(&b).map_err(|e| match e {
        LutIoError::Format(ref msg) if msg.contains("grid sizes differ") => {
            // Extract grid sizes from error message for backward-compatible error type
            let parts: Vec<u32> = msg
                .split(" vs ")
                .filter_map(|s| s.split_whitespace().last()?.parse().ok())
                .collect();
            if parts.len() == 2 {
                LutDiffError::GridSizeMismatch {
                    a: parts[0],
                    b: parts[1],
                }
            } else {
                LutDiffError::ParseError(e)
            }
        }
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
#[derive(Debug, thiserror::Error)]
pub enum LutDepError {
    /// Missing required argument.
    #[error("LUTDEP_MISSING_ARG -- {0}")]
    MissingArg(String),
    /// Database initialization failed.
    #[error("LUTDEP_DB_ERROR -- {0}")]
    DbError(String),
    /// Database query error.
    #[error("LUTDEP_DB_ERROR -- {0}")]
    QueryError(String),
}

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
    use crate::format_traits::LutData;
    use rusqlite::Connection;

    /// Mock LutIo that writes/reads .cube files directly (no Format crate dependency).
    struct MockCubeLutIo;

    impl LutIo for MockCubeLutIo {
        fn parse_lut(&self, path: &Path) -> Result<LutData, LutIoError> {
            let data = std::fs::read_to_string(path)?;
            parse_cube_from_str(&data)
        }

        fn serialize_lut(&self, data: &LutData, path: &Path) -> Result<(), LutIoError> {
            let content = serialize_cube_to_str(data)?;
            std::fs::write(path, content)?;
            Ok(())
        }
    }

    fn parse_cube_from_str(content: &str) -> Result<LutData, LutIoError> {
        let mut grid_size: Option<u32> = None;
        let mut values: Vec<f64> = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with('#')
                || trimmed.starts_with("TITLE")
                || trimmed.starts_with("DOMAIN_MIN")
                || trimmed.starts_with("DOMAIN_MAX")
            {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("LUT_3D_SIZE") {
                grid_size = Some(rest.trim().parse().map_err(|_| {
                    LutIoError::Parse(format!("invalid LUT_3D_SIZE: {}", rest.trim()))
                })?);
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                let parsed: Result<Vec<f64>, _> =
                    parts.iter().map(|s| s.parse::<f64>()).collect();
                if let Ok(floats) = parsed {
                    values.extend_from_slice(&floats[..3]);
                }
            }
        }

        let grid_size = grid_size.ok_or_else(|| LutIoError::Parse("LUT_3D_SIZE not found".to_string()))?;
        let expected = grid_size as usize * grid_size as usize * grid_size as usize * 3;
        if values.len() != expected {
            return Err(LutIoError::Format(format!(
                "expected {} values, got {}",
                expected,
                values.len()
            )));
        }

        Ok(LutData {
            title: None,
            grid_size,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
            values,
        })
    }

    fn serialize_cube_to_str(data: &LutData) -> Result<String, LutIoError> {
        data.validate()?;
        let mut out = String::new();
        out.push_str(&format!("LUT_3D_SIZE {}\n", data.grid_size));
        for chunk in data.values.chunks(3) {
            out.push_str(&format!("{:.6} {:.6} {:.6}\n", chunk[0], chunk[1], chunk[2]));
        }
        Ok(out)
    }

    fn setup_test_db() -> Connection {
        crate::test_db::setup_test_db()
    }

    #[test]
    fn test_compare_luts_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.cube");
        MockCubeLutIo
            .serialize_lut(&LutData::identity(5), &path)
            .unwrap();

        let result = compare_luts(&MockCubeLutIo, &path, &path).unwrap();
        assert_eq!(result.total_points, 125);
        for ch in 0..3 {
            assert!(result.channels[ch].mean_delta < 1e-15);
            assert_eq!(result.channels[ch].changed_count, 0);
        }
    }

    #[test]
    fn test_compare_luts_file_not_found() {
        let result = compare_luts(
            &MockCubeLutIo,
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
