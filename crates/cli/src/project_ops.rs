// project_ops.rs — I/O orchestration functions that depend on Format crate.
// Moved from Core (Phase 2a) to respect dependency direction: cli → core → format.

use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use mengxi_core::color_science;
use mengxi_core::downsample::{self, MAX_DIMENSION};
use mengxi_core::fingerprint::{self, FingerprintError};
use mengxi_core::project::{Project, VariantBreakdown};

// Re-export ImportError so CLI match arms can use project_ops::ImportError
pub use mengxi_core::project::ImportError;

use mengxi_core::format_traits::{LutData, LutIo, LutIoError};
use mengxi_format::dpx;
use mengxi_format::exr as exr_format;
use mengxi_format::lut as format_lut;
use mengxi_format::mov as mov_format;

// ---------------------------------------------------------------------------
// register_project (moved from crates/core/src/project.rs)
// ---------------------------------------------------------------------------

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

/// Register a project: scan files, decode DPX headers, check for duplicates, insert into DB.
/// Returns the project record and a variant breakdown for display.
/// If project already exists, resumes import (skips already-processed files).
pub fn register_project(
    conn: &Connection,
    name: &str,
    path: &Path,
    tile_grid_size: u32,
    mut on_progress: impl FnMut(usize, usize, &str),
) -> Result<(Project, VariantBreakdown), ImportError> {
    // Check for existing project (resume support)
    let (project_id, is_new) = match conn.query_row(
        "SELECT id FROM projects WHERE name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(id) => (id, false),
        Err(rusqlite::Error::QueryReturnedNoRows) => (0, true),
        Err(e) => return Err(ImportError::DbError(e.to_string())),
    };

    // Scan files first to get counts
    let files = scan_project_files(path)?;
    let dpx_count = files.iter().filter(|(_, f)| f == "dpx").count() as i64;
    let exr_count = files.iter().filter(|(_, f)| f == "exr").count() as i64;
    let mov_count = files.iter().filter(|(_, f)| f == "mov").count() as i64;

    if is_new {
        // Insert project record
        conn.execute(
            "INSERT INTO projects (name, path, dpx_count, exr_count, mov_count) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, path.to_string_lossy(), dpx_count, exr_count, mov_count],
        )
        .map_err(|e| ImportError::DbError(e.to_string()))?;
    }

    // Get project_id (either from existing or just-inserted)
    let project_id = if is_new {
        conn.last_insert_rowid()
    } else {
        project_id
    };

    // Query already-imported filenames for resume support (use HashSet for O(1) lookup)
    let existing_files: HashSet<String> = if !is_new {
        let mut stmt = conn.prepare(
            "SELECT filename FROM files WHERE project_id = ?1",
        ).map_err(|e| ImportError::DbError(e.to_string()))?;
        let result: HashSet<String> = stmt.query_map([project_id], |row| row.get::<_, String>(0))
            .map_err(|e| ImportError::DbError(e.to_string()))?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(|e| ImportError::DbError(e.to_string()))?;
        result
    } else {
        HashSet::new()
    };

    // Begin transaction for atomic import
    let tx = conn.unchecked_transaction()
        .map_err(|e| ImportError::DbError(e.to_string()))?;

    // Insert file records with DPX metadata extraction
    let mut variant_counts: HashMap<String, usize> = HashMap::new();
    let mut breakdown = VariantBreakdown::default();
    let new_files_count = files.len() - existing_files.len();

    for (filename, format) in &files {
        // Skip already-imported files (resume support)
        if existing_files.contains(filename) {
            breakdown.resumed_count += 1;
            continue;
        }

        let file_path = path.join(filename);
        let processed = breakdown.skipped_count + 1; // 1-indexed among new files
        on_progress(processed, new_files_count, filename);

        // Extract format-specific metadata
        let (width, height, bit_depth, transfer, colorimetric, descriptor, compression, codec, fps, duration, frame_count) = if format == "dpx" {
            match dpx::parse_dpx_header(&file_path) {
                Ok(header) => {
                    let t = dpx::transfer_to_string(header.transfer).to_string();
                    let d = dpx::descriptor_to_string(header.descriptor).to_string();
                    let variant_key = format!("{}-bit {}", header.bit_depth, t);
                    *variant_counts.entry(variant_key).or_insert(0) += 1;

                    (
                        Some(header.width as i64),
                        Some(header.height as i64),
                        Some(header.bit_depth as i64),
                        Some(t),
                        Some(dpx::colorimetric_to_string(header.colorimetric).to_string()),
                        Some(d),
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                }
                Err(e) => {
                    breakdown.skipped_count += 1;
                    breakdown.skipped_files.push(filename.clone());
                    eprintln!("Error: {}", ImportError::CorruptFile {
                        filename: filename.clone(),
                        reason: e.to_string(),
                    });
                    continue;
                }
            }
        } else if format == "exr" {
            match exr_format::parse_exr_header(&file_path) {
                Ok(header) => {
                    let pt = exr_format::pixel_type_to_string(&header.pixel_type);
                    let comp_str = exr_format::compression_to_db_string(&header.compression);
                    let desc = exr_format::channels_to_descriptor(&header.channels);
                    let variant_key = format!("{} {}", pt, header.compression.to_display());
                    *variant_counts.entry(variant_key).or_insert(0) += 1;

                    (
                        Some(header.width as i64),
                        Some(header.height as i64),
                        Some(exr_format::pixel_type_to_bit_depth(&header.pixel_type) as i64),
                        Some("linear".to_string()),
                        None,
                        Some(desc),
                        Some(comp_str.to_string()),
                        None,
                        None,
                        None,
                        None,
                    )
                }
                Err(e) => {
                    breakdown.skipped_count += 1;
                    breakdown.skipped_files.push(filename.clone());
                    eprintln!("Error: {}", ImportError::CorruptFile {
                        filename: filename.clone(),
                        reason: e.to_string(),
                    });
                    continue;
                }
            }
        } else if format == "mov" {
            match mov_format::parse_mov_header(&file_path) {
                Ok(header) => {
                    let variant_key = mov_format::codec_to_variant_key(&header.codec);
                    *variant_counts.entry(variant_key).or_insert(0) += 1;

                    (
                        Some(header.width as i64),
                        Some(header.height as i64),
                        header.bit_depth.map(|bd| bd as i64),
                        None,
                        None,
                        None,
                        None,
                        Some(header.codec),
                        Some(header.fps),
                        Some(header.duration_secs),
                        Some(header.frame_count as i64),
                    )
                }
                Err(e) => {
                    breakdown.skipped_count += 1;
                    breakdown.skipped_files.push(filename.clone());
                    eprintln!("Error: {}", ImportError::CorruptFile {
                        filename: filename.clone(),
                        reason: e.to_string(),
                    });
                    continue;
                }
            }
        } else {
            (None, None, None, None, None, None, None, None, None, None, None)
        };

        conn.execute(
            "INSERT OR IGNORE INTO files (project_id, filename, format, width, height, bit_depth, transfer, colorimetric, descriptor, compression, codec, fps, duration, frame_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![project_id, filename, format, width, height, bit_depth, transfer, colorimetric, descriptor, compression, codec, fps, duration, frame_count],
        )
        .map_err(|e| ImportError::DbError(e.to_string()))?;

        let file_id = conn.last_insert_rowid();

        // Extract color fingerprint for DPX and EXR files
        if format == "dpx" || format == "exr" {
            let color_tag = mengxi_core::feature_pipeline::determine_color_tag(format, transfer.as_deref());

            let pixel_result = if format == "dpx" {
                dpx::read_pixel_data(&file_path)
                    .map_err(|e| ImportError::CorruptFile {
                        filename: filename.clone(),
                        reason: e.to_string(),
                    })
            } else {
                exr_format::read_pixel_data(&file_path)
                    .map_err(|e| ImportError::CorruptFile {
                        filename: filename.clone(),
                        reason: e.to_string(),
                    })
            };

            if let Ok(pixel_data) = pixel_result {
                // Downsample for feature extraction (NFR2: 4K → ~512, ~34x reduction)
                let w = width.and_then(|v| if v > 0 { Some(v as usize) } else { None });
                let h = height.and_then(|v| if v > 0 { Some(v as usize) } else { None });
                let (downsampled_data, ds_w, ds_h) = match (w, h) {
                    (Some(w), Some(h)) => {
                        match downsample::downsample_rgb(&pixel_data, w, h, MAX_DIMENSION) {
                            Ok(data) => data,
                            Err(e) => {
                                eprintln!("Warning: downsampling failed for {}: {}", filename, e);
                                continue;
                            }
                        }
                    }
                    _ => (pixel_data.clone(), 0usize, 0usize),
                };

                match fingerprint::extract_fingerprint(&downsampled_data, &color_tag) {
                    Ok(fp) => {
                        let hist_r = fp.histogram_r.iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        let hist_g = fp.histogram_g.iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        let hist_b = fp.histogram_b.iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(",");

                        if let Err(e) = conn.execute(
                            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                            params![file_id, hist_r, hist_g, hist_b, fp.luminance_mean, fp.luminance_stddev, fp.color_space_tag],
                        ) {
                            eprintln!("Warning: failed to store fingerprint for {}: {}", filename, e);
                        } else {
                            breakdown.fingerprint_count += 1;

                            // Extract Oklab grading features from downsampled data
                            match mengxi_core::feature_pipeline::extract_features_from_pixels(
                                &downsampled_data,
                                None, // already downsampled
                                None,
                                &color_tag,
                            ) {
                                Ok(features) => {
                                    let hist_l_blob = features.hist_l_blob();
                                    let hist_a_blob = features.hist_a_blob();
                                    let hist_b_blob = features.hist_b_blob();
                                    let moments_blob = features.moments_blob();
                                    if let Err(e) = conn.execute(
                                        "UPDATE fingerprints SET oklab_hist_l = ?1, oklab_hist_a = ?2, oklab_hist_b = ?3, color_moments = ?4, hist_bins = ?5, feature_status = 'fresh' WHERE file_id = ?6",
                                        params![hist_l_blob, hist_a_blob, hist_b_blob, moments_blob, color_science::GradingFeatures::HIST_BINS as i32, file_id],
                                    ) {
                                        eprintln!("Warning: failed to store grading features for {}: {}", filename, e);
                                    } else {
                                        breakdown.grading_feature_count += 1;

                                        // Extract per-tile features if tile_grid_size configured
                                        if tile_grid_size > 0 && ds_w > 0 && ds_h > 0 {
                                            let fingerprint_id = conn.last_insert_rowid();
                                            // Convert RGB → Oklab for tile extraction
                                            match color_science::rgb_to_oklab_batch(&downsampled_data, &color_tag) {
                                                Ok(oklab_data) => {
                                                    match mengxi_core::feature_pipeline::extract_tile_features(
                                                        &oklab_data, ds_w, ds_h, &color_tag,
                                                        tile_grid_size as usize,
                                                        color_science::GradingFeatures::HIST_BINS,
                                                    ) {
                                                        Ok(tiles) => {
                                                            if let Err(e) = mengxi_core::db::store_fingerprint_tiles(
                                                                conn, fingerprint_id, &tiles,
                                                                color_science::GradingFeatures::HIST_BINS,
                                                            ) {
                                                                eprintln!("Warning: failed to store tile features for {}: {}", filename, e);
                                                            }
                                                        }
                                                        Err(e) => {
                                                            eprintln!("Warning: tile feature extraction failed for {}: {}", filename, e);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    eprintln!("Warning: tile Oklab conversion failed for {}: {}", filename, e);
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Warning: grading feature extraction failed for {}: {}", filename, e);
                                }
                            }
                        }
                    }
                    Err(FingerprintError::FfiUnavailable) => {
                        // Silently skip — FFI not linked
                    }
                    Err(FingerprintError::FfiError(code, ctx)) => {
                        eprintln!("Warning: fingerprint FFI error (code={}) for {}: {}", code, filename, ctx);
                    }
                    Err(FingerprintError::InvalidInput(msg)) => {
                        eprintln!("Warning: fingerprint invalid input for {}: {}", filename, msg);
                    }
                }
            }
        }
    }

    // Build variant breakdown string
    let mut sorted_variants: Vec<_> = variant_counts.into_iter().collect();
    sorted_variants.sort_by(|a, b| b.1.cmp(&a.1));
    for (variant, count) in sorted_variants {
        breakdown.variants.push(format!("{}x {}", count, variant));
    }

    // Update project counts to reflect actual imported files
    conn.execute(
        "UPDATE projects SET dpx_count = ?1, exr_count = ?2, mov_count = ?3 WHERE id = ?4",
        params![dpx_count, exr_count, mov_count, project_id],
    )
    .map_err(|e| ImportError::DbError(e.to_string()))?;

    // Commit transaction
    tx.commit().map_err(|e| ImportError::DbError(e.to_string()))?;

    // Load project record from DB for accurate created_at
    let project = conn.query_row(
        "SELECT id, name, path, dpx_count, exr_count, mov_count, created_at FROM projects WHERE id = ?1",
        [project_id],
        |row| Ok(Project {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            dpx_count: row.get(3)?,
            exr_count: row.get(4)?,
            mov_count: row.get(5)?,
            created_at: row.get(6)?,
        }),
    ).map_err(|e| ImportError::DbError(e.to_string()))?;

    Ok((project, breakdown))
}

// ---------------------------------------------------------------------------
// reextract_grading_features (moved from crates/core/src/fingerprint.rs)
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
    let color_tag = mengxi_core::feature_pipeline::determine_color_tag(&format, transfer.as_deref());

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
    let features = match mengxi_core::feature_pipeline::extract_features_from_pixels(
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
        rusqlite::params![hist_l_blob, hist_a_blob, hist_b_blob, moments_blob, mengxi_core::color_science::GradingFeatures::HIST_BINS as i32, fingerprint_id],
    ) {
        return Ok(ReextractResult::Error(format!("REEXTRACT_DB_ERROR -- {}", e)));
    }

    // Extract per-tile features if tile_grid_size configured
    if tile_grid_size > 0 {
        // Get downsampled data for tile extraction
        let (ds_data, ds_w, ds_h) = if let (Some(w), Some(h)) = (w, h) {
            match mengxi_core::downsample::downsample_rgb(&pixel_data, w, h, mengxi_core::downsample::MAX_DIMENSION) {
                Ok((data, dw, dh)) => (data, dw, dh),
                Err(_) => return Ok(ReextractResult::Reextracted),
            }
        } else {
            // No dimensions — can't tile
            return Ok(ReextractResult::Reextracted);
        };

        match mengxi_core::color_science::rgb_to_oklab_batch(&ds_data, &color_tag) {
            Ok(oklab_data) => {
                match mengxi_core::feature_pipeline::extract_tile_features(
                    &oklab_data, ds_w, ds_h, &color_tag,
                    tile_grid_size as usize,
                    mengxi_core::color_science::GradingFeatures::HIST_BINS,
                ) {
                    Ok(tiles) => {
                        if let Err(e) = mengxi_core::db::store_fingerprint_tiles(
                            conn, fingerprint_id, &tiles,
                            mengxi_core::color_science::GradingFeatures::HIST_BINS,
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
        .query_map([file_path], |row| {
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

// ---------------------------------------------------------------------------
// LUT I/O bridge (Phase 2b) — implements Core's LutIo trait using Format crate
// ---------------------------------------------------------------------------

/// Bridge that implements `mengxi_core::format_traits::LutIo` by delegating to
/// `mengxi_format::lut` functions. This is the CLI-layer implementation that
/// allows Core to remain independent of the Format crate.
pub struct MengxiFormatLutBridge;

impl LutIo for MengxiFormatLutBridge {
    fn parse_lut(&self, path: &std::path::Path) -> Result<LutData, LutIoError> {
        let data = format_lut::parse_lut(path).map_err(|e| LutIoError::Parse(e.to_string()))?;
        Ok(LutData {
            title: data.title,
            grid_size: data.grid_size,
            domain_min: data.domain_min,
            domain_max: data.domain_max,
            values: data.values,
        })
    }

    fn serialize_lut(&self, data: &LutData, path: &std::path::Path) -> Result<(), LutIoError> {
        let inner = format_lut::LutData {
            title: data.title.clone(),
            grid_size: data.grid_size,
            domain_min: data.domain_min,
            domain_max: data.domain_max,
            values: data.values.clone(),
        };
        format_lut::serialize_lut(&inner, path).map_err(|e| LutIoError::Serialize(e.to_string()))
    }
}
