// project_ops.rs — I/O orchestration functions for the agent crate.
// Duplicated from CLI/project_ops.rs because the agent is a separate process
// that cannot call CLI code. Both crates depend on mengxi-format directly.

use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use mengxi_core::color_science;
use mengxi_core::downsample::{self, MAX_DIMENSION};
use mengxi_core::fingerprint;
use mengxi_core::project::{ImportError, Project, VariantBreakdown};

use mengxi_format::dpx;
use mengxi_format::exr as exr_format;
use mengxi_format::mov as mov_format;

/// Supported file extensions and their format labels.
const SUPPORTED_EXTENSIONS: &[(&str, &str)] = &[
    ("dpx", "dpx"),
    ("exr", "exr"),
    ("mov", "mov"),
    ("DPX", "dpx"),
    ("EXR", "exr"),
    ("MOV", "mov"),
];

/// Scan a directory for supported media files.
pub fn scan_project_files(path: &Path) -> Result<Vec<(String, String)>, ImportError> {
    if !path.exists() {
        return Err(ImportError::PathNotFound(path.to_string_lossy().to_string()));
    }

    let mut files = Vec::new();
    let entries = fs::read_dir(path).map_err(|e| ImportError::DbError(e.to_string()))?;

    for entry in entries {
        let entry = entry.map_err(|e| ImportError::DbError(e.to_string()))?;
        let path = entry.path();
        if !path.is_file() { continue; }

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

/// Register a project (agent version — mirrors CLI/project_ops.rs).
pub fn register_project(
    conn: &Connection,
    name: &str,
    path: &Path,
    _tile_grid_size: u32,
    mut on_progress: impl FnMut(usize, usize, &str),
) -> Result<(Project, VariantBreakdown), ImportError> {
    let (project_id, is_new) = match conn.query_row(
        "SELECT id FROM projects WHERE name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(id) => (id, false),
        Err(rusqlite::Error::QueryReturnedNoRows) => (0, true),
        Err(e) => return Err(ImportError::DbError(e.to_string())),
    };

    let files = scan_project_files(path)?;
    let dpx_count = files.iter().filter(|(_, f)| f == "dpx").count() as i64;
    let exr_count = files.iter().filter(|(_, f)| f == "exr").count() as i64;
    let mov_count = files.iter().filter(|(_, f)| f == "mov").count() as i64;

    if is_new {
        conn.execute(
            "INSERT INTO projects (name, path, dpx_count, exr_count, mov_count) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, path.to_string_lossy(), dpx_count, exr_count, mov_count],
        ).map_err(|e| ImportError::DbError(e.to_string()))?;
    }

    let project_id = if is_new { conn.last_insert_rowid() } else { project_id };

    let existing_files: HashSet<String> = if !is_new {
        let mut result = HashSet::new();
        let mut stmt = conn.prepare("SELECT filename FROM files WHERE project_id = ?1")
            .map_err(|e| ImportError::DbError(e.to_string()))?;
        let rows = stmt.query_map([project_id], |row| row.get::<_, String>(0))
            .map_err(|e| ImportError::DbError(e.to_string()))?;
        for row in rows {
            result.insert(row.map_err(|e| ImportError::DbError(e.to_string()))?);
        }
        result
    } else {
        HashSet::new()
    };

    let tx = conn.unchecked_transaction().map_err(|e| ImportError::DbError(e.to_string()))?;

    let mut variant_counts: HashMap<String, usize> = HashMap::new();
    let mut breakdown = VariantBreakdown::default();
    let new_files_count = files.len() - existing_files.len();

    for (filename, format) in &files {
        if existing_files.contains(filename) {
            breakdown.resumed_count += 1;
            continue;
        }

        let file_path = path.join(filename);
        let processed = breakdown.skipped_count + 1;
        on_progress(processed, new_files_count, filename);

        let (width, height, bit_depth, transfer, colorimetric, descriptor, compression, codec, fps, duration, frame_count) = if format == "dpx" {
            match dpx::parse_dpx_header(&file_path) {
                Ok(header) => {
                    let t = dpx::transfer_to_string(header.transfer).to_string();
                    let d = dpx::descriptor_to_string(header.descriptor).to_string();
                    let variant_key = format!("{}-bit {}", header.bit_depth, t);
                    *variant_counts.entry(variant_key).or_insert(0) += 1;
                    (
                        Some(header.width as i64), Some(header.height as i64),
                        Some(header.bit_depth as i64), Some(t),
                        Some(dpx::colorimetric_to_string(header.colorimetric).to_string()),
                        Some(d), None, None, None, None, None,
                    )
                }
                Err(_) => {
                    breakdown.skipped_count += 1;
                    breakdown.skipped_files.push(filename.clone());
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
                        Some(header.width as i64), Some(header.height as i64),
                        Some(exr_format::pixel_type_to_bit_depth(&header.pixel_type) as i64),
                        Some("linear".to_string()), None, Some(desc),
                        Some(comp_str.to_string()), None, None, None, None,
                    )
                }
                Err(_) => {
                    breakdown.skipped_count += 1;
                    breakdown.skipped_files.push(filename.clone());
                    continue;
                }
            }
        } else if format == "mov" {
            match mov_format::parse_mov_header(&file_path) {
                Ok(header) => {
                    let variant_key = mov_format::codec_to_variant_key(&header.codec);
                    *variant_counts.entry(variant_key).or_insert(0) += 1;
                    (
                        Some(header.width as i64), Some(header.height as i64),
                        header.bit_depth.map(|bd| bd as i64),
                        None, None, None, None,
                        Some(header.codec), Some(header.fps),
                        Some(header.duration_secs), Some(header.frame_count as i64),
                    )
                }
                Err(_) => {
                    breakdown.skipped_count += 1;
                    breakdown.skipped_files.push(filename.clone());
                    continue;
                }
            }
        } else {
            (None, None, None, None, None, None, None, None, None, None, None)
        };

        conn.execute(
            "INSERT OR IGNORE INTO files (project_id, filename, format, width, height, bit_depth, transfer, colorimetric, descriptor, compression, codec, fps, duration, frame_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![project_id, filename, format, width, height, bit_depth, transfer, colorimetric, descriptor, compression, codec, fps, duration, frame_count],
        ).map_err(|e| ImportError::DbError(e.to_string()))?;

        let file_id = conn.last_insert_rowid();

        if format == "dpx" || format == "exr" {
            let color_tag = mengxi_core::feature_pipeline::determine_color_tag(format, transfer.as_deref());

            let pixel_result = if format == "dpx" {
                dpx::read_pixel_data(&file_path)
                    .map_err(|e| ImportError::CorruptFile { filename: filename.clone(), reason: e.to_string() })
            } else {
                exr_format::read_pixel_data(&file_path)
                    .map_err(|e| ImportError::CorruptFile { filename: filename.clone(), reason: e.to_string() })
            };

            if let Ok(pixel_data) = pixel_result {
                let w = width.and_then(|v| if v > 0 { Some(v as usize) } else { None });
                let h = height.and_then(|v| if v > 0 { Some(v as usize) } else { None });
                let (downsampled_data, _ds_w, _ds_h) = match (w, h) {
                    (Some(w), Some(h)) => {
                        match downsample::downsample_rgb(&pixel_data, w, h, MAX_DIMENSION) {
                            Ok(data) => data,
                            Err(_) => continue,
                        }
                    }
                    _ => (pixel_data.clone(), 0usize, 0usize),
                };

                if let Ok(fp) = fingerprint::extract_fingerprint(&downsampled_data, &color_tag) {
                    let hist_r = fp.histogram_r.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
                    let hist_g = fp.histogram_g.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
                    let hist_b = fp.histogram_b.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");

                    if conn.execute(
                        "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![file_id, hist_r, hist_g, hist_b, fp.luminance_mean, fp.luminance_stddev, fp.color_space_tag],
                    ).is_ok() {
                        breakdown.fingerprint_count += 1;

                        if let Ok(features) = mengxi_core::feature_pipeline::extract_features_from_pixels(
                            &downsampled_data, None, None, &color_tag,
                        ) {
                            let hist_l_blob = features.hist_l_blob();
                            let hist_a_blob = features.hist_a_blob();
                            let hist_b_blob = features.hist_b_blob();
                            let moments_blob = features.moments_blob();
                            let _ = conn.execute(
                                "UPDATE fingerprints SET oklab_hist_l=?1,oklab_hist_a=?2,oklab_hist_b=?3,color_moments=?4,hist_bins=?5,feature_status='fresh' WHERE file_id=?6",
                                params![hist_l_blob, hist_a_blob, hist_b_blob, moments_blob, color_science::GradingFeatures::HIST_BINS as i32, file_id],
                            );
                            // Skip tile extraction in agent (tile_grid_size=0)
                        }
                    }
                }
            }
        }
    }

    let mut sorted_variants: Vec<_> = variant_counts.into_iter().collect();
    sorted_variants.sort_by(|a, b| b.1.cmp(&a.1));
    for (variant, count) in sorted_variants {
        breakdown.variants.push(format!("{}x {}", count, variant));
    }

    conn.execute(
        "UPDATE projects SET dpx_count=?1,exr_count=?2,mov_count=?3 WHERE id=?4",
        params![dpx_count, exr_count, mov_count, project_id],
    ).map_err(|e| ImportError::DbError(e.to_string()))?;

    tx.commit().map_err(|e| ImportError::DbError(e.to_string()))?;

    let project = conn.query_row(
        "SELECT id,name,path,dpx_count,exr_count,mov_count,created_at FROM projects WHERE id=?1",
        [project_id],
        |row| Ok(Project { id: row.get(0)?, name: row.get(1)?, path: row.get(2)?, dpx_count: row.get(3)?, exr_count: row.get(4)?, mov_count: row.get(5)?, created_at: row.get(6)? }),
    ).map_err(|e| ImportError::DbError(e.to_string()))?;

    Ok((project, breakdown))
}

// --- Re-extraction types and functions ---

#[derive(Debug, Clone, PartialEq)]
pub enum ReextractResult {
    Reextracted,
    Skipped(String),
    Error(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ReextractError {
    #[error("REEXTRACT_DB_ERROR -- {0}")]
    DbError(String),
}

pub fn reextract_grading_features(
    conn: &rusqlite::Connection,
    fingerprint_id: i64,
    _tile_grid_size: u32,
) -> Result<ReextractResult, ReextractError> {
    let (file_path, format, transfer, width, height): (String, String, Option<String>, Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT p.path||'/'||f.filename,f.format,f.transfer,f.width,f.height FROM fingerprints fp JOIN files f ON fp.file_id=f.id JOIN projects p ON f.project_id=p.id WHERE fp.id=?1",
            rusqlite::params![fingerprint_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|e| ReextractError::DbError(e.to_string()))?;

    if format == "mov" {
        return Ok(ReextractResult::Skipped("MOV files have no pixel data".to_string()));
    }
    if format != "dpx" && format != "exr" {
        return Ok(ReextractResult::Skipped(format!("unsupported format: {}", format)));
    }

    let color_tag = mengxi_core::feature_pipeline::determine_color_tag(&format, transfer.as_deref());

    if !std::path::Path::new(&file_path).is_file() {
        return Ok(ReextractResult::Skipped(format!("source file not found: {}", file_path)));
    }

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

    let w = width.and_then(|v| if v > 0 { Some(v as usize) } else { None });
    let h = height.and_then(|v| if v > 0 { Some(v as usize) } else { None });

    let features = match mengxi_core::feature_pipeline::extract_features_from_pixels(&pixel_data, w, h, &color_tag) {
        Ok(f) => f,
        Err(e) => return Ok(ReextractResult::Error(format!("REEXTRACT_PIPELINE_ERROR -- {}", e))),
    };

    let hist_l_blob = features.hist_l_blob();
    let hist_a_blob = features.hist_a_blob();
    let hist_b_blob = features.hist_b_blob();
    let moments_blob = features.moments_blob();

    if let Err(e) = conn.execute(
        "UPDATE fingerprints SET oklab_hist_l=?1,oklab_hist_a=?2,oklab_hist_b=?3,color_moments=?4,hist_bins=?5,feature_status='fresh' WHERE id=?6",
        rusqlite::params![hist_l_blob, hist_a_blob, hist_b_blob, moments_blob, mengxi_core::color_science::GradingFeatures::HIST_BINS as i32, fingerprint_id],
    ) {
        return Ok(ReextractResult::Error(format!("REEXTRACT_DB_ERROR -- {}", e)));
    }

    Ok(ReextractResult::Reextracted)
}

pub fn list_fingerprints_by_project(
    conn: &rusqlite::Connection,
    project_name: &str,
) -> Result<Vec<(i64, String)>, ReextractError> {
    let mut stmt = conn.prepare(
        "SELECT fp.id,p.path||'/'||f.filename FROM fingerprints fp JOIN files f ON fp.file_id=f.id JOIN projects p ON f.project_id=p.id WHERE p.name=?1 ORDER BY fp.id",
    ).map_err(|e| ReextractError::DbError(e.to_string()))?;
    let rows = stmt.query_map(rusqlite::params![project_name], |row| Ok((row.get::<_,i64>(0)?, row.get::<_,String>(1)?)))
        .map_err(|e| ReextractError::DbError(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ReextractError::DbError(e.to_string()))?;
    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct BatchReextractResult {
    pub reextracted: usize,
    pub skipped: usize,
    pub failed: usize,
    pub failures: Vec<(String, String)>,
}

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
    let mut result = BatchReextractResult { reextracted: 0, skipped: 0, failed: 0, failures: Vec::new() };

    conn.execute_batch("BEGIN TRANSACTION").map_err(|e| ReextractError::DbError(format!("failed to begin transaction: {}", e)))?;

    for (i, (fp_id, fp_path)) in fingerprint_ids.iter().enumerate() {
        progress(i, total, fp_path);
        match reextract_grading_features(conn, *fp_id, tile_grid_size) {
            Ok(ReextractResult::Reextracted) => result.reextracted += 1,
            Ok(ReextractResult::Skipped(_)) => result.skipped += 1,
            Ok(ReextractResult::Error(reason)) => {
                result.failed += 1;
                result.failures.push((fp_path.clone(), reason));
            }
            Err(e) => {
                result.failed += 1;
                result.failures.push((fp_path.clone(), e.to_string()));
            }
        }
    }

    conn.execute_batch("COMMIT").map_err(|e| ReextractError::DbError(format!("failed to commit transaction: {}", e)))?;
    Ok(result)
}
