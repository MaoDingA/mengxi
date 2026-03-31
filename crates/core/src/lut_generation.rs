// lut_generation.rs — Core LUT export orchestration
// Bridges DB ↔ FFI ↔ Format layers

use crate::color_science::{generate_lut, ACESColorSpace, ColorScienceError};
use rusqlite::Connection;
use std::path::PathBuf;

use mengxi_format::lut::{
    self, LutData, LutFormat, LutError,
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for LUT export.
#[derive(Debug, Clone)]
pub struct ExportLutConfig {
    pub project_id: i64,
    pub fingerprint_id: Option<i64>,
    pub format: String,
    pub output_path: PathBuf,
    pub grid_size: u32,
    pub force: bool,
    pub interactive: bool,
}

impl ExportLutConfig {
    pub fn new(project_id: i64, output_path: PathBuf, format: String) -> Self {
        ExportLutConfig {
            project_id,
            fingerprint_id: None,
            format,
            output_path,
            grid_size: 33,
            force: false,
            interactive: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

/// Result of a successful LUT export.
#[derive(Debug, Clone)]
pub struct LutExportResult {
    pub path: PathBuf,
    pub grid_size: u32,
    pub format: String,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from LUT generation and export.
#[derive(Debug, thiserror::Error)]
pub enum LutGenerationError {
    /// No fingerprint found for the given project/fingerprint_id.
    #[error("EXPORT_FINGERPRINT_NOT_FOUND -- no fingerprint data found for this project")]
    FingerprintNotFound,
    /// FFI error from color science engine.
    #[error("EXPORT_FFI_ERROR -- {0}")]
    FfiError(#[from] ColorScienceError),
    /// LUT format error (validation, serialization).
    #[error("EXPORT_FORMAT_ERROR -- {0}")]
    FormatError(#[from] LutError),
    /// File write error.
    #[error("EXPORT_WRITE_ERROR -- {0}")]
    WriteError(String),
    /// File already exists and overwrite was denied.
    #[error("EXPORT_FILE_EXISTS -- {} already exists. Use --force to overwrite.", .0.display())]
    OverwriteDenied(PathBuf),
    /// Overwrite prompt required in scripted mode.
    #[error("EXPORT_FILE_EXISTS -- {}", .0.display())]
    FileExists(PathBuf),
}

// ---------------------------------------------------------------------------
// Export logic
// ---------------------------------------------------------------------------

/// Export a LUT for a project's fingerprint.
///
/// Steps: validate config → check overwrite → query fingerprint → generate LUT
/// → validate → serialize → write file → record in DB.
pub fn export_lut(
    conn: &Connection,
    config: &ExportLutConfig,
) -> Result<LutExportResult, LutGenerationError> {
    // Check file existence
    if config.output_path.exists() && !config.force {
        if config.interactive {
            // In interactive mode, the CLI handles the prompt.
            // We return FileExists to let CLI prompt the user.
            return Err(LutGenerationError::FileExists(config.output_path.clone()));
        } else {
            return Err(LutGenerationError::OverwriteDenied(config.output_path.clone()));
        }
    }

    // Determine source color space from fingerprint
    let (src_cs, title) = resolve_color_space(conn, config)?;

    // Validate format
    let lut_fmt = LutFormat::from_extension(&config.format)
        .map_err(LutGenerationError::FormatError)?;

    // Reject CDL format (parametric, not a 3D LUT)
    if matches!(lut_fmt, LutFormat::AscCdl) {
        return Err(LutError::UnsupportedFormat(
            "ASC-CDL is parametric and cannot be exported as a 3D LUT".to_string(),
        ).into());
    }

    // Validate path extension matches format
    if let Some(ext) = config.output_path.extension().and_then(|e| e.to_str()) {
        if ext.to_lowercase() != config.format.to_lowercase() {
            return Err(LutError::UnsupportedFormat(
                format!("output path extension '.{}' does not match specified format '{}'", ext, config.format)
            ).into());
        }
    }

    // Generate LUT values via MoonBit FFI
    let dst = ACESColorSpace::Rec709;
    let values = generate_lut(config.grid_size, src_cs, dst)?;

    // Build LutData
    let lut_data = LutData {
        title: title.clone(),
        grid_size: config.grid_size,
        domain_min: [0.0, 0.0, 0.0],
        domain_max: [1.0, 1.0, 1.0],
        values,
    };

    // FR41: Pre-write validation
    lut_data.validate()?;

    // Serialize and write
    // Ensure parent directory exists
    if let Some(parent) = config.output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| LutGenerationError::WriteError(e.to_string()))?;
    }

    lut::serialize_lut(&lut_data, &config.output_path)
        .map_err(LutGenerationError::FormatError)?;

    // Record in DB
    record_export(conn, config, title.as_deref())?;

    Ok(LutExportResult {
        path: config.output_path.clone(),
        grid_size: config.grid_size,
        format: config.format.clone(),
    })
}

/// Force-overwrite export (used after user confirms in interactive mode).
pub fn export_lut_force(
    conn: &Connection,
    mut config: ExportLutConfig,
) -> Result<LutExportResult, LutGenerationError> {
    config.force = true;
    export_lut(conn, &config)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve the source color space and optional title from the project's fingerprint.
fn resolve_color_space(
    conn: &Connection,
    config: &ExportLutConfig,
) -> Result<(ACESColorSpace, Option<String>), LutGenerationError> {
    let fp_sql = if config.fingerprint_id.is_some() {
        "SELECT color_space_tag FROM fingerprints WHERE id = ?1"
    } else {
        "SELECT color_space_tag FROM fingerprints WHERE file_id IN (SELECT id FROM files WHERE project_id = ?1) LIMIT 1"
    };

    let fp_id_param: i64 = config.fingerprint_id.unwrap_or(config.project_id);

    let result = conn.query_row(fp_sql, [fp_id_param], |row| {
        row.get::<_, String>(0)
    });

    match result {
        Ok(color_space_tag) => {
            let src_cs = ACESColorSpace::parse(&color_space_tag);
            if src_cs.is_log() {
                Ok((ACESColorSpace::ACEScg, Some(format!("LUT: {}", config.format))))
            } else {
                Ok((src_cs, Some(format!("LUT: {}", config.format))))
            }
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            if config.fingerprint_id.is_some() {
                Err(LutGenerationError::FingerprintNotFound)
            } else {
                Ok((ACESColorSpace::ACEScg, None))
            }
        }
        Err(e) => Err(LutGenerationError::WriteError(format!(
            "database error: {}",
            e
        ))),
    }
}

/// Record a LUT export in the database.
fn record_export(
    conn: &Connection,
    config: &ExportLutConfig,
    title: Option<&str>,
) -> Result<(), LutGenerationError> {
    conn.execute(
        "INSERT INTO luts (project_id, fingerprint_id, format, grid_size, output_path, title)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            config.project_id,
            config.fingerprint_id,
            config.format,
            config.grid_size as i64,
            config.output_path.to_string_lossy().to_string(),
            title,
        ],
    )
    .map_err(|e| LutGenerationError::WriteError(format!("database insert failed: {}", e)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        // Create required tables
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
    fn test_export_lut_no_fingerprint() {
        let conn = setup_test_db();
        // Insert a project with no fingerprint
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("test.cube");
        let config = ExportLutConfig::new(1, output, "cube".to_string());

        let result = export_lut(&conn, &config);
        // Should succeed even without fingerprint (defaults to ACEScg)
        assert!(result.is_ok());
        let export = result.unwrap();
        assert_eq!(export.grid_size, 33);
        assert!(export.path.exists());

        // Verify file is parseable
        let lut = lut::parse_lut(&export.path).unwrap();
        assert_eq!(lut.grid_size, 33);
    }

    #[test]
    fn test_export_lut_with_fingerprint() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files (project_id, filename, format) VALUES (1, 'test.dpx', 'dpx')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '[]', '[]', '[]', 0.5, 0.1, 'acescg')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("test.cube");
        let config = ExportLutConfig::new(1, output, "cube".to_string());

        let result = export_lut(&conn, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_export_lut_overwrite_denied() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("test.cube");

        // First export
        let config = ExportLutConfig::new(1, output.clone(), "cube".to_string());
        assert!(export_lut(&conn, &config).is_ok());

        // Second export without force — should fail
        assert!(output.exists());
        let result = export_lut(&conn, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            LutGenerationError::OverwriteDenied(p) => {
                assert_eq!(p, output);
            }
            other => panic!("Expected OverwriteDenied, got: {:?}", other),
        }
    }

    #[test]
    fn test_export_lut_overwrite_forced() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("test.cube");

        // First export
        let mut config = ExportLutConfig::new(1, output.clone(), "cube".to_string());
        assert!(export_lut(&conn, &config).is_ok());

        // Second export with force
        config.force = true;
        assert!(export_lut(&conn, &config).is_ok());
    }

    #[test]
    fn test_export_lut_interactive_file_exists() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("test.cube");

        // First export
        let mut config = ExportLutConfig::new(1, output.clone(), "cube".to_string());
        assert!(export_lut(&conn, &config).is_ok());

        // Interactive mode with existing file
        config.interactive = true;
        let result = export_lut(&conn, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            LutGenerationError::FileExists(p) => {
                assert_eq!(p, output);
            }
            other => panic!("Expected FileExists, got: {:?}", other),
        }
    }

    #[test]
    fn test_export_lut_creates_parent_dir() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("subdir1").join("subdir2").join("test.cube");
        let config = ExportLutConfig::new(1, output.clone(), "cube".to_string());

        let result = export_lut(&conn, &config);
        assert!(result.is_ok());
        assert!(output.exists());
    }

    #[test]
    fn test_export_lut_db_record() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("test.cube");
        let config = ExportLutConfig::new(1, output.clone(), "cube".to_string());

        export_lut(&conn, &config).unwrap();

        // Verify DB record
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM luts WHERE project_id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);

        let grid_size: i64 = conn
            .query_row("SELECT grid_size FROM luts WHERE project_id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(grid_size, 33);
    }

    #[test]
    fn test_export_multiple_formats() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();

        for ext in &["cube", "3dl", "look", "csp"] {
            let output = dir.path().join(format!("test.{}", ext));
            let config = ExportLutConfig::new(1, output.clone(), ext.to_string());
            let result = export_lut(&conn, &config);
            assert!(result.is_ok(), "format {} failed: {:?}", ext, result.err());
            assert!(output.exists());
        }
    }

    #[test]
    fn test_error_display() {
        let err = LutGenerationError::FingerprintNotFound;
        assert!(format!("{}", err).contains("EXPORT_FINGERPRINT_NOT_FOUND"));

        let err = LutGenerationError::OverwriteDenied(PathBuf::from("/tmp/test.cube"));
        assert!(format!("{}", err).contains("EXPORT_FILE_EXISTS"));

        let err = LutGenerationError::FileExists(PathBuf::from("/tmp/test.cube"));
        assert!(format!("{}", err).contains("EXPORT_FILE_EXISTS"));

        let err = LutGenerationError::WriteError("disk full".to_string());
        assert!(format!("{}", err).contains("EXPORT_WRITE_ERROR"));
    }

    #[test]
    fn test_export_lut_cdl_rejected() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("test.cdl");
        let config = ExportLutConfig::new(1, output, "cdl".to_string());

        let result = export_lut(&conn, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            LutGenerationError::FormatError(_) => {}
            other => panic!("Expected FormatError for CDL, got: {:?}", other),
        }
    }

    #[test]
    fn test_export_lut_fingerprint_id_not_found() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("test.cube");
        let mut config = ExportLutConfig::new(1, output, "cube".to_string());
        config.fingerprint_id = Some(999);

        let result = export_lut(&conn, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            LutGenerationError::FingerprintNotFound => {}
            other => panic!("Expected FingerprintNotFound, got: {:?}", other),
        }
    }

    #[test]
    fn test_export_lut_path_extension_mismatch() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("test.dat");
        let config = ExportLutConfig::new(1, output, "cube".to_string());

        let result = export_lut(&conn, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            LutGenerationError::FormatError(_) => {}
            other => panic!("Expected FormatError for extension mismatch, got: {:?}", other),
        }
    }

    #[test]
    fn test_export_lut_db_record_with_title() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files (project_id, filename, format) VALUES (1, 'test.dpx', 'dpx')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '[]', '[]', '[]', 0.5, 0.1, 'acescg')",
            [],
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("test.cube");
        let config = ExportLutConfig::new(1, output, "cube".to_string());

        export_lut(&conn, &config).unwrap();

        // Verify title is stored in DB
        let title: Option<String> = conn
            .query_row("SELECT title FROM luts WHERE project_id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(title.is_some());
        assert_eq!(title.unwrap(), "LUT: cube");
    }
}
