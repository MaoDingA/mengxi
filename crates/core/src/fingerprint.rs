// fingerprint.rs — FFI bridge to MoonBit color fingerprint extraction

use mengxi_format::dpx;
use mengxi_format::exr as exr_format;

/// Number of histogram bins per channel.
pub const BINS_PER_CHANNEL: usize = 64;

/// Total output size: 64 bins R + 64 bins G + 64 bins B + mean + stddev.
pub const OUTPUT_SIZE: usize = BINS_PER_CHANNEL * 3 + 2;

/// Color space tag at the FFI boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpaceTag {
    Linear,
    Log,
    Video,
}

impl ColorSpaceTag {
    pub fn parse(s: &str) -> Self {
        match s {
            "log" => ColorSpaceTag::Log,
            "video" => ColorSpaceTag::Video,
            _ => ColorSpaceTag::Linear,
        }
    }

    pub fn as_int(&self) -> i32 {
        match self {
            ColorSpaceTag::Linear => 0,
            ColorSpaceTag::Log => 1,
            ColorSpaceTag::Video => 2,
        }
    }
}

/// Extracted color fingerprint from a file.
#[derive(Debug, Clone)]
pub struct Fingerprint {
    pub histogram_r: Vec<f64>,
    pub histogram_g: Vec<f64>,
    pub histogram_b: Vec<f64>,
    pub luminance_mean: f64,
    pub luminance_stddev: f64,
    pub color_space_tag: String,
}

/// Errors from fingerprint extraction.
#[derive(Debug, thiserror::Error)]
pub enum FingerprintError {
    /// MoonBit library not available (not linked).
    #[error("FINGERPRINT_UNAVAILABLE -- MoonBit FFI library not linked")]
    FfiUnavailable,
    /// MoonBit returned an error code.
    #[error("FINGERPRINT_FFI_ERROR -- code {0} for {1}")]
    FfiError(i32, String),
    /// Invalid input data.
    #[error("FINGERPRINT_INVALID_INPUT -- {0}")]
    InvalidInput(String),
}

extern "C" {
    fn mengxi_compute_fingerprint(
        data_len: i32,
        data_ptr: *const f64,
        color_tag: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

/// Extract color fingerprint from interleaved RGB pixel data via MoonBit FFI.
///
/// # Arguments
/// * `pixel_data` — Interleaved RGB values normalized to [0.0, 1.0], length must be divisible by 3.
/// * `color_space_tag` — Color space: "linear", "log", or "video".
///
/// # Returns
/// * `Ok(Fingerprint)` on success with histogram bins and luminance statistics.
/// * `Err(FingerprintError)` if data is invalid or MoonBit returns an error.
pub fn extract_fingerprint(
    pixel_data: &[f64],
    color_space_tag: &str,
) -> Result<Fingerprint, FingerprintError> {
    if pixel_data.len() < 3 {
        return Err(FingerprintError::InvalidInput(
            "pixel data must contain at least 3 values (1 pixel)".to_string(),
        ));
    }
    if pixel_data.len() > i32::MAX as usize {
        return Err(FingerprintError::InvalidInput(
            format!("pixel data too large for FFI ({} elements, max {})", pixel_data.len(), i32::MAX),
        ));
    }
    if !pixel_data.len().is_multiple_of(3) {
        return Err(FingerprintError::InvalidInput(
            "pixel data length must be divisible by 3 (RGB)".to_string(),
        ));
    }

    let tag = ColorSpaceTag::parse(color_space_tag);
    let mut output = vec![0.0_f64; OUTPUT_SIZE];

    let result = unsafe {
        mengxi_compute_fingerprint(
            pixel_data.len() as i32,
            pixel_data.as_ptr(),
            tag.as_int(),
            OUTPUT_SIZE as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(FingerprintError::FfiError(
            result,
            color_space_tag.to_string(),
        ));
    }

    let histogram_r = output[0..BINS_PER_CHANNEL].to_vec();
    let histogram_g = output[BINS_PER_CHANNEL..BINS_PER_CHANNEL * 2].to_vec();
    let histogram_b = output[BINS_PER_CHANNEL * 2..BINS_PER_CHANNEL * 3].to_vec();
    let luminance_mean = output[BINS_PER_CHANNEL * 3];
    let luminance_stddev = output[BINS_PER_CHANNEL * 3 + 1];

    Ok(Fingerprint {
        histogram_r,
        histogram_g,
        histogram_b,
        luminance_mean,
        luminance_stddev,
        color_space_tag: color_space_tag.to_string(),
    })
}

/// Check if MoonBit FFI is available by testing a trivial call.
/// Returns true if the library is linked and responsive.
pub fn is_ffi_available() -> bool {
    let data = [0.5_f64, 0.5, 0.5];
    let mut output = [0.0_f64; OUTPUT_SIZE];
    let result = unsafe {
        mengxi_compute_fingerprint(
            3,
            data.as_ptr(),
            ColorSpaceTag::Linear.as_int(),
            OUTPUT_SIZE as i32,
            output.as_mut_ptr(),
        )
    };
    result == OUTPUT_SIZE as i32
}

// ---------------------------------------------------------------------------
// Grading feature re-extraction (Story 5.4)
// ---------------------------------------------------------------------------

/// Result of a single fingerprint re-extraction attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum ReextractResult {
    /// Features were successfully re-extracted and stored.
    Reextracted,
    /// Fingerprint was skipped (source file missing, MOV file, etc.).
    Skipped(String),
    /// Re-extraction failed (FFI error, DB error, etc.).
    Error(String),
}

/// Errors from re-extraction that prevent the operation from being attempted.
#[derive(Debug, thiserror::Error)]
pub enum ReextractError {
    /// Database query failed (fingerprint not found, connection error).
    #[error("REEXTRACT_DB_ERROR -- {0}")]
    DbError(String),
}

/// Re-extract grading features for a single fingerprint by reading its source file.
///
/// Looks up the source file path via `files` + `projects` tables, reads pixel data,
/// downsamples, converts RGB→Oklab, extracts features via FFI, and atomically
/// updates the 4 BLOB columns + feature_status.
pub fn reextract_grading_features(
    conn: &rusqlite::Connection,
    fingerprint_id: i64,
    tile_grid_size: u32,
) -> Result<ReextractResult, ReextractError> {
    // Look up file metadata: path, format, transfer, dimensions, project path
    let (file_path, format, transfer, width, height): (String, String, Option<String>, Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT p.path || '/' || f.filename, f.format, f.transfer, f.width, f.height
             FROM fingerprints fp
             JOIN files f ON fp.file_id = f.id
             JOIN projects p ON f.project_id = p.id
             WHERE fp.id = ?1",
            rusqlite::params![fingerprint_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|e| ReextractError::DbError(e.to_string()))?;

    // Skip MOV files — no pixel data
    if format == "mov" {
        return Ok(ReextractResult::Skipped(
            "MOV files have no pixel data".to_string(),
        ));
    }

    // Only DPX and EXR are supported for pixel data reading
    if format != "dpx" && format != "exr" {
        return Ok(ReextractResult::Skipped(format!(
            "unsupported format: {}",
            format
        )));
    }

    // Determine color space tag
    let color_tag = crate::feature_pipeline::determine_color_tag(&format, transfer.as_deref());

    // Check source file exists
    if !std::path::Path::new(&file_path).is_file() {
        return Ok(ReextractResult::Skipped(format!(
            "source file not found: {}",
            file_path
        )));
    }

    // Read pixel data
    let path = std::path::Path::new(&file_path);
    let pixel_data = if format == "dpx" {
        match dpx::read_pixel_data(path) {
            Ok(data) => data,
            Err(e) => return Ok(ReextractResult::Error(format!("REEXTRACT_READ_ERROR -- {}", e))),
        }
    } else {
        match exr_format::read_pixel_data(path) {
            Ok(data) => data,
            Err(e) => return Ok(ReextractResult::Error(format!("REEXTRACT_READ_ERROR -- {}", e))),
        }
    };

    // Downsample + RGB→Oklab + FFI feature extraction via shared pipeline
    let w = width.and_then(|v| if v > 0 { Some(v as usize) } else { None });
    let h = height.and_then(|v| if v > 0 { Some(v as usize) } else { None });
    let features = match crate::feature_pipeline::extract_features_from_pixels(
        &pixel_data, w, h, &color_tag,
    ) {
        Ok(f) => f,
        Err(e) => return Ok(ReextractResult::Error(format!("REEXTRACT_PIPELINE_ERROR -- {}", e))),
    };

    // Serialize to BLOBs
    let hist_l_blob = features.hist_l_blob();
    let hist_a_blob = features.hist_a_blob();
    let hist_b_blob = features.hist_b_blob();
    let moments_blob = features.moments_blob();

    // Atomic UPDATE: all 4 BLOBs + hist_bins + feature_status in one statement
    if let Err(e) = conn.execute(
        "UPDATE fingerprints
         SET oklab_hist_l = ?1, oklab_hist_a = ?2, oklab_hist_b = ?3, color_moments = ?4, hist_bins = ?5, feature_status = 'fresh'
         WHERE id = ?6",
        rusqlite::params![hist_l_blob, hist_a_blob, hist_b_blob, moments_blob, crate::color_science::GradingFeatures::HIST_BINS as i32, fingerprint_id],
    ) {
        return Ok(ReextractResult::Error(format!("REEXTRACT_DB_ERROR -- {}", e)));
    }

    // Extract per-tile features if tile_grid_size configured
    if tile_grid_size > 0 {
        // Get downsampled data for tile extraction
        let (ds_data, ds_w, ds_h) = if let (Some(w), Some(h)) = (w, h) {
            match crate::downsample::downsample_rgb(&pixel_data, w, h, crate::downsample::MAX_DIMENSION) {
                Ok((data, dw, dh)) => (data, dw, dh),
                Err(_) => return Ok(ReextractResult::Reextracted),
            }
        } else {
            // No dimensions — can't tile
            return Ok(ReextractResult::Reextracted);
        };

        match crate::color_science::rgb_to_oklab_batch(&ds_data, &color_tag) {
            Ok(oklab_data) => {
                match crate::feature_pipeline::extract_tile_features(
                    &oklab_data, ds_w, ds_h, &color_tag,
                    tile_grid_size as usize,
                    crate::color_science::GradingFeatures::HIST_BINS,
                ) {
                    Ok(tiles) => {
                        if let Err(e) = crate::db::store_fingerprint_tiles(
                            conn, fingerprint_id, &tiles,
                            crate::color_science::GradingFeatures::HIST_BINS,
                        ) {
                            eprintln!("Warning: failed to store tile features during re-extraction: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: tile feature extraction failed during re-extraction: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: tile Oklab conversion failed during re-extraction: {}", e);
            }
        }
    }

    Ok(ReextractResult::Reextracted)
}

/// Query all fingerprint IDs for a given project name.
pub fn list_fingerprints_by_project(
    conn: &rusqlite::Connection,
    project_name: &str,
) -> Result<Vec<(i64, String)>, ReextractError> {
    let mut stmt = conn
        .prepare(
            "SELECT fp.id, p.path || '/' || f.filename
             FROM fingerprints fp
             JOIN files f ON fp.file_id = f.id
             JOIN projects p ON f.project_id = p.id
             WHERE p.name = ?1
             ORDER BY fp.id",
        )
        .map_err(|e| ReextractError::DbError(e.to_string()))?;

    let mut rows = Vec::new();
    let mut rows_iter = stmt
        .query_map(rusqlite::params![project_name], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| ReextractError::DbError(e.to_string()))?;

    for row in &mut rows_iter {
        rows.push(row.map_err(|e| ReextractError::DbError(e.to_string()))?);
    }

    Ok(rows)
}

/// Query all fingerprint IDs for a given file path.
pub fn list_fingerprints_by_file(
    conn: &rusqlite::Connection,
    file_path: &str,
) -> Result<Vec<(i64, String)>, ReextractError> {
    let mut stmt = conn
        .prepare(
            "SELECT fp.id, p.path || '/' || f.filename
             FROM fingerprints fp
             JOIN files f ON fp.file_id = f.id
             JOIN projects p ON f.project_id = p.id
             WHERE p.path || '/' || f.filename = ?1
             ORDER BY fp.id",
        )
        .map_err(|e| ReextractError::DbError(e.to_string()))?;

    let mut rows = Vec::new();
    let mut rows_iter = stmt
        .query_map(rusqlite::params![file_path], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| ReextractError::DbError(e.to_string()))?;

    for row in &mut rows_iter {
        rows.push(row.map_err(|e| ReextractError::DbError(e.to_string()))?);
    }

    Ok(rows)
}

/// Batch re-extraction result summary.
#[derive(Debug, Clone)]
pub struct BatchReextractResult {
    pub reextracted: usize,
    pub skipped: usize,
    pub failed: usize,
    pub failures: Vec<(String, String)>,
}

/// Re-extract grading features for multiple fingerprints in a single transaction.
///
/// Wraps all individual `reextract_grading_features` calls in a SQLite transaction,
/// providing significant performance improvement for large batches by reducing
/// WAL checkpoint overhead from N commits to 1.
///
/// The `progress` callback is called before each fingerprint with `(current, total, path)`.
pub fn batch_reextract_grading_features<F>(
    conn: &rusqlite::Connection,
    fingerprint_ids: &[(i64, String)],
    tile_grid_size: u32,
    mut progress: F,
) -> Result<BatchReextractResult, ReextractError>
where
    F: FnMut(usize, usize, &str),
{
    let total = fingerprint_ids.len();
    let mut result = BatchReextractResult {
        reextracted: 0,
        skipped: 0,
        failed: 0,
        failures: Vec::new(),
    };

    conn.execute_batch("BEGIN TRANSACTION")
        .map_err(|e| ReextractError::DbError(format!("failed to begin transaction: {}", e)))?;

    for (i, (fp_id, fp_path)) in fingerprint_ids.iter().enumerate() {
        progress(i, total, fp_path);
        match reextract_grading_features(conn, *fp_id, tile_grid_size) {
            Ok(ReextractResult::Reextracted) => result.reextracted += 1,
            Ok(ReextractResult::Skipped(reason)) => {
                result.skipped += 1;
                eprintln!("  skipped: {}", reason);
            }
            Ok(ReextractResult::Error(reason)) => {
                result.failed += 1;
                eprintln!("  error: {}", reason);
                result.failures.push((fp_path.clone(), reason));
            }
            Err(e) => {
                result.failed += 1;
                eprintln!("  error: {}", e);
                result.failures.push((fp_path.clone(), e.to_string()));
            }
        }
    }

    conn.execute_batch("COMMIT")
        .map_err(|e| ReextractError::DbError(format!("failed to commit transaction: {}", e)))?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_space_tag_from_str() {
        assert_eq!(ColorSpaceTag::parse("linear"), ColorSpaceTag::Linear);
        assert_eq!(ColorSpaceTag::parse("log"), ColorSpaceTag::Log);
        assert_eq!(ColorSpaceTag::parse("video"), ColorSpaceTag::Video);
        assert_eq!(ColorSpaceTag::parse("unknown"), ColorSpaceTag::Linear);
    }

    #[test]
    fn test_color_space_tag_as_int() {
        assert_eq!(ColorSpaceTag::Linear.as_int(), 0);
        assert_eq!(ColorSpaceTag::Log.as_int(), 1);
        assert_eq!(ColorSpaceTag::Video.as_int(), 2);
    }

    #[test]
    fn test_extract_fingerprint_too_few_pixels() {
        let result = extract_fingerprint(&[0.5], "linear");
        assert!(result.is_err());
        match result.unwrap_err() {
            FingerprintError::InvalidInput(msg) => {
                assert!(msg.contains("at least 3"));
            }
            other => panic!("Expected InvalidInput, got: {:?}", other),
        }
    }

    #[test]
    fn test_extract_fingerprint_not_divisible_by_3() {
        let result = extract_fingerprint(&[0.5, 0.5, 0.5, 0.5], "linear");
        assert!(result.is_err());
        match result.unwrap_err() {
            FingerprintError::InvalidInput(msg) => {
                assert!(msg.contains("divisible by 3"));
            }
            other => panic!("Expected InvalidInput, got: {:?}", other),
        }
    }

    #[test]
    fn test_extract_fingerprint_uniform_color() {
        // Single pixel, all channels at 0.5
        let data = [0.5_f64, 0.5, 0.5];
        let fp = extract_fingerprint(&data, "linear").unwrap();

        assert_eq!(fp.histogram_r.len(), BINS_PER_CHANNEL);
        assert_eq!(fp.histogram_g.len(), BINS_PER_CHANNEL);
        assert_eq!(fp.histogram_b.len(), BINS_PER_CHANNEL);
        assert_eq!(fp.color_space_tag, "linear");
        // All values at 0.5 should land in bin 32 (0-indexed)
        assert_eq!(fp.histogram_r[32], 1.0);
        assert_eq!(fp.histogram_g[32], 1.0);
        assert_eq!(fp.histogram_b[32], 1.0);
        // Luminance of (0.5, 0.5, 0.5) = 0.5
        assert!((fp.luminance_mean - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_extract_fingerprint_two_pixels() {
        // Red pixel + Green pixel
        let data = [
            1.0_f64, 0.0, 0.0, // red
            0.0_f64, 1.0, 0.0, // green
        ];
        let fp = extract_fingerprint(&data, "linear").unwrap();

        // Red channel: one at 1.0 (bin 63), one at 0.0 (bin 0)
        assert_eq!(fp.histogram_r[63], 0.5);
        assert_eq!(fp.histogram_r[0], 0.5);
        // Green channel: one at 0.0 (bin 0), one at 1.0 (bin 63)
        assert_eq!(fp.histogram_g[0], 0.5);
        assert_eq!(fp.histogram_g[63], 0.5);
        // Blue channel: both at 0.0 (bin 0)
        assert_eq!(fp.histogram_b[0], 1.0);
    }

    #[test]
    fn test_extract_fingerprint_output_buffer_too_small() {
        let data = [0.5_f64, 0.5, 0.5];
        let mut output = [0.0_f64; 10];
        let result = unsafe {
            mengxi_compute_fingerprint(
                3,
                data.as_ptr(),
                0,
                10,
                output.as_mut_ptr(),
            )
        };
        assert_eq!(result, -2);
    }

    #[test]
    fn test_is_ffi_available() {
        assert!(is_ffi_available());
    }

    #[test]
    fn test_fingerprint_error_display() {
        let err = FingerprintError::FfiUnavailable;
        let msg = format!("{}", err);
        assert!(msg.contains("FINGERPRINT_UNAVAILABLE"));

        let err = FingerprintError::FfiError(-1, "test".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("FINGERPRINT_FFI_ERROR"));

        let err = FingerprintError::InvalidInput("bad data".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("FINGERPRINT_INVALID_INPUT"));
    }

    // --- Re-extraction tests (Story 5.4) ---

    fn setup_test_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, path TEXT NOT NULL, dpx_count INTEGER NOT NULL DEFAULT 0, exr_count INTEGER NOT NULL DEFAULT 0, mov_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE, filename TEXT NOT NULL, format TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()), width INTEGER, height INTEGER, bit_depth INTEGER, transfer TEXT, colorimetric TEXT, descriptor TEXT, compression TEXT);
             CREATE TABLE fingerprints (id INTEGER PRIMARY KEY AUTOINCREMENT, file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE, histogram_r TEXT, histogram_g TEXT, histogram_b TEXT, luminance_mean REAL, luminance_stddev REAL, color_space_tag TEXT, embedding BLOB, oklab_hist_l BLOB, oklab_hist_a BLOB, oklab_hist_b BLOB, color_moments BLOB, feature_status TEXT CHECK(feature_status IN ('fresh', 'stale') OR feature_status IS NULL), created_at INTEGER NOT NULL DEFAULT (unixepoch()));",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_reextract_mov_file_skipped() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/film')", []).unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'clip.mov', 'mov')", []).unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, feature_status) VALUES (1, 'stale')", []).unwrap();

        let result = reextract_grading_features(&conn, 1, 0).unwrap();
        assert_eq!(result, ReextractResult::Skipped("MOV files have no pixel data".to_string()));
    }

    #[test]
    fn test_reextract_missing_source_skipped() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/nonexistent/path')", []).unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'missing.dpx', 'dpx')", []).unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, feature_status) VALUES (1, 'stale')", []).unwrap();

        let result = reextract_grading_features(&conn, 1, 0).unwrap();
        assert!(matches!(result, ReextractResult::Skipped(reason) if reason.contains("source file not found")));
    }

    #[test]
    fn test_reextract_nonexistent_fingerprint_errors() {
        let conn = setup_test_db();
        let result = reextract_grading_features(&conn, 999, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("REEXTRACT_DB_ERROR"));
    }

    #[test]
    fn test_reextract_unsupported_format_skipped() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/film')", []).unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'img.ari', 'ari')", []).unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, feature_status) VALUES (1, 'stale')", []).unwrap();

        let result = reextract_grading_features(&conn, 1, 0).unwrap();
        assert!(matches!(result, ReextractResult::Skipped(reason) if reason.contains("unsupported format")));
    }

    #[test]
    fn test_reextract_error_display() {
        let err = ReextractError::DbError("test".to_string());
        assert!(err.to_string().contains("REEXTRACT_DB_ERROR"));
    }

    #[test]
    fn test_list_fingerprints_by_project() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/film')", []).unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'a.dpx', 'dpx')", []).unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'b.exr', 'exr')", []).unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, feature_status) VALUES (1, 'fresh')", []).unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, feature_status) VALUES (2, 'stale')", []).unwrap();

        let fps = list_fingerprints_by_project(&conn, "film").unwrap();
        assert_eq!(fps.len(), 2);
        assert_eq!(fps[0].0, 1);
        assert_eq!(fps[0].1, "/tmp/film/a.dpx");
        assert_eq!(fps[1].0, 2);
        assert_eq!(fps[1].1, "/tmp/film/b.exr");
    }

    #[test]
    fn test_list_fingerprints_by_project_empty() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/film')", []).unwrap();

        let fps = list_fingerprints_by_project(&conn, "film").unwrap();
        assert!(fps.is_empty());
    }

    #[test]
    fn test_list_fingerprints_by_file() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/film')", []).unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'a.dpx', 'dpx')", []).unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, feature_status) VALUES (1, 'fresh')", []).unwrap();

        let fps = list_fingerprints_by_file(&conn, "/tmp/film/a.dpx").unwrap();
        assert_eq!(fps.len(), 1);
        assert_eq!(fps[0].0, 1);
    }

    #[test]
    fn test_list_fingerprints_by_file_not_found() {
        let conn = setup_test_db();
        let fps = list_fingerprints_by_file(&conn, "/nonexistent/file.dpx").unwrap();
        assert!(fps.is_empty());
    }

    #[test]
    fn test_reextract_result_equality() {
        assert_eq!(ReextractResult::Reextracted, ReextractResult::Reextracted);
        assert_eq!(ReextractResult::Skipped("x".to_string()), ReextractResult::Skipped("x".to_string()));
        assert_ne!(ReextractResult::Skipped("a".to_string()), ReextractResult::Skipped("b".to_string()));
    }
}
