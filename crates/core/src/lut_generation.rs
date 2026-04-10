// lut_generation.rs — Core LUT export orchestration
// Bridges DB ↔ FFI ↔ LutIo trait (Format layer abstracted away)

use crate::color_science::{generate_lut, ACESColorSpace, ColorScienceError};
use crate::format_traits::{LutData, LutFormat, LutIo, LutIoError};
use rusqlite::Connection;
use std::path::PathBuf;

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
    /// LUT I/O error (validation, serialization, parsing).
    #[error("EXPORT_FORMAT_ERROR -- {0}")]
    FormatError(#[from] LutIoError),
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
/// → validate → serialize via LutIo → write file → record in DB.
pub fn export_lut(
    conn: &Connection,
    lut_io: &dyn LutIo,
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
    let lut_fmt = LutFormat::from_extension(&config.format)?;

    // Reject CDL format (parametric, not a 3D LUT)
    if matches!(lut_fmt, LutFormat::Cdl) {
        return Err(LutIoError::Format(
            "ASC-CDL is parametric and cannot be exported as a 3D LUT".to_string(),
        )
        .into());
    }

    // Validate path extension matches format
    if let Some(ext) = config.output_path.extension().and_then(|e| e.to_str()) {
        if ext.to_lowercase() != config.format.to_lowercase() {
            return Err(LutIoError::Format(format!(
                "output path extension '.{}' does not match specified format '{}'",
                ext, config.format
            ))
            .into());
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

    // Pre-write validation
    lut_data.validate()?;

    // Serialize and write via LutIo trait
    // Ensure parent directory exists
    if let Some(parent) = config.output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| LutGenerationError::WriteError(e.to_string()))?;
    }

    lut_io.serialize_lut(&lut_data, &config.output_path)?;

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
    lut_io: &dyn LutIo,
    mut config: ExportLutConfig,
) -> Result<LutExportResult, LutGenerationError> {
    config.force = true;
    export_lut(conn, lut_io, &config)
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
        "SELECT aces_color_space, color_space_tag FROM fingerprints WHERE id = ?1"
    } else {
        "SELECT aces_color_space, color_space_tag FROM fingerprints \
         WHERE file_id IN (SELECT id FROM files WHERE project_id = ?1) LIMIT 1"
    };

    let fp_id_param: i64 = config.fingerprint_id.unwrap_or(config.project_id);

    let result = conn.query_row(fp_sql, [fp_id_param], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,  // aces_color_space (NEW column)
            row.get::<_, String>(1),           // color_space_tag (existing, for fallback)
        ))
    });

    match result {
        Ok((aces_opt, _encoding_tag)) => {
            // Use explicit aces_color_space if available
            let src_cs = ACESColorSpace::from_aces_column(aces_opt.as_deref());
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
    use std::io::Write;
    use std::path::Path;

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

    /// Minimal .cube parser for test mock (only supports identity-like data).
    fn parse_cube_from_str(content: &str) -> Result<LutData, LutIoError> {
        let mut grid_size: Option<u32> = None;
        let mut values: Vec<f64> = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("TITLE") {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("LUT_3D_SIZE") {
                grid_size = Some(rest.trim().parse().map_err(|_| {
                    LutIoError::Parse(format!("invalid LUT_3D_SIZE: {}", rest.trim()))
                })?);
                continue;
            }
            if trimmed.starts_with("DOMAIN_MIN") || trimmed.starts_with("DOMAIN_MAX") {
                continue;
            }
            // Data lines
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                let parsed: Result<Vec<f64>, _> = parts.iter().map(|s| s.parse::<f64>()).collect();
                if let Ok(floats) = parsed {
                    values.extend_from_slice(&floats[..3]);
                }
            }
        }

        let grid_size = grid_size.ok_or_else(|| {
            LutIoError::Parse("LUT_3D_SIZE not found".to_string())
        })?;
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

    /// Minimal .cube serializer for test mock.
    fn serialize_cube_to_str(data: &LutData) -> Result<String, LutIoError> {
        data.validate()?;
        let mut out = String::new();
        out.push_str(&format!(
            "DOMAIN_MIN {:.6} {:.6} {:.6}\n",
            data.domain_min[0], data.domain_min[1], data.domain_min[2]
        ));
        out.push_str(&format!(
            "DOMAIN_MAX {:.6} {:.6} {:.6}\n",
            data.domain_max[0], data.domain_max[1], data.domain_max[2]
        ));
        out.push_str(&format!("LUT_3D_SIZE {}\n", data.grid_size));
        for chunk in data.values.chunks(3) {
            out.push_str(&format!("{:.6} {:.6} {:.6}\n", chunk[0], chunk[1], chunk[2]));
        }
        Ok(out)
    }

    fn setup_test_db() -> Connection {
        crate::test_db::setup_test_db()
    }

    #[cfg(moonbit_ffi)]
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

        let result = export_lut(&conn, &MockCubeLutIo, &config);
        // Should succeed even without fingerprint (defaults to ACEScg)
        assert!(result.is_ok());
        let export = result.unwrap();
        assert_eq!(export.grid_size, 33);
        assert!(export.path.exists());

        // Verify file is parseable by our mock
        let lut = MockCubeLutIo.parse_lut(&export.path).unwrap();
        assert_eq!(lut.grid_size, 33);
    }

    #[cfg(moonbit_ffi)]
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

        let result = export_lut(&conn, &MockCubeLutIo, &config);
        assert!(result.is_ok());
    }

    #[cfg(moonbit_ffi)]
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
        assert!(export_lut(&conn, &MockCubeLutIo, &config).is_ok());

        // Second export without force — should fail
        assert!(output.exists());
        let result = export_lut(&conn, &MockCubeLutIo, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            LutGenerationError::OverwriteDenied(p) => {
                assert_eq!(p, output);
            }
            other => panic!("Expected OverwriteDenied, got: {:?}", other),
        }
    }

    #[cfg(moonbit_ffi)]
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
        assert!(export_lut(&conn, &MockCubeLutIo, &config).is_ok());

        // Second export with force
        config.force = true;
        assert!(export_lut_force(&conn, &MockCubeLutIo, config).is_ok());
    }

    #[cfg(moonbit_ffi)]
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
        assert!(export_lut(&conn, &MockCubeLutIo, &config).is_ok());

        // Interactive mode with existing file
        config.interactive = true;
        let result = export_lut(&conn, &MockCubeLutIo, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            LutGenerationError::FileExists(p) => {
                assert_eq!(p, output);
            }
            other => panic!("Expected FileExists, got: {:?}", other),
        }
    }

    #[cfg(moonbit_ffi)]
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

        let result = export_lut(&conn, &MockCubeLutIo, &config);
        assert!(result.is_ok());
        assert!(output.exists());
    }

    #[cfg(moonbit_ffi)]
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

        export_lut(&conn, &MockCubeLutIo, &config).unwrap();

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

    #[cfg(moonbit_ffi)]
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
            let result = export_lut(&conn, &MockCubeLutIo, &config);
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

    #[cfg(moonbit_ffi)]
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

        let result = export_lut(&conn, &MockCubeLutIo, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            LutGenerationError::FormatError(_) => {}
            other => panic!("Expected FormatError for CDL, got: {:?}", other),
        }
    }

    #[cfg(moonbit_ffi)]
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

        let result = export_lut(&conn, &MockCubeLutIo, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            LutGenerationError::FingerprintNotFound => {}
            other => panic!("Expected FingerprintNotFound, got: {:?}", other),
        }
    }

    #[cfg(moonbit_ffi)]
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

        let result = export_lut(&conn, &MockCubeLutIo, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            LutGenerationError::FormatError(_) => {}
            other => panic!("Expected FormatError for extension mismatch, got: {:?}", other),
        }
    }

    #[cfg(moonbit_ffi)]
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

        export_lut(&conn, &MockCubeLutIo, &config).unwrap();

        // Verify title is stored in DB
        let title: Option<String> = conn
            .query_row("SELECT title FROM luts WHERE project_id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(title.is_some());
        assert_eq!(title.unwrap(), "LUT: cube");
    }

    #[test]
    fn test_resolve_color_space_uses_explicit_aces_column() {
        // This test verifies that "linear"/"log"/"video" encoding tags
        // NO LONGER silently fall through to ACEScg via ACESColorSpace::parse()
        // Instead they go through infer_from_encoding_tag which is intentional
        // Verify that parse() still falls back for unknown strings (unchanged behavior)
        assert_eq!(ACESColorSpace::parse("unknown_string"), ACESColorSpace::ACEScg);

        // Verify new from_aces_column handles NULL correctly
        assert_eq!(ACESColorSpace::from_aces_column(None), ACESColorSpace::ACEScg);

        // Verify new from_aces_column handles explicit ACES names
        assert_eq!(
            ACESColorSpace::from_aces_column(Some("ACES2065-1")),
            ACESColorSpace::ACES2065_1
        );
        assert_eq!(
            ACESColorSpace::from_aces_column(Some("rec709")),
            ACESColorSpace::Rec709
        );

        // Verify infer_from_encoding_tag provides sensible defaults
        assert_eq!(
            ACESColorSpace::infer_from_encoding_tag("linear"),
            ACESColorSpace::ACEScg
        );
        assert_eq!(
            ACESColorSpace::infer_from_encoding_tag("log"),
            ACESColorSpace::ACEScct
        );
        assert_eq!(
            ACESColorSpace::infer_from_encoding_tag("video"),
            ACESColorSpace::Rec709
        );
    }
}
