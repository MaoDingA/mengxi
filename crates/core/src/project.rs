use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use mengxi_format::dpx;
use mengxi_format::exr as exr_format;
use mengxi_format::mov as mov_format;

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
}

/// Error types for import operations.
#[derive(Debug)]
pub enum ImportError {
    PathNotFound(String),
    DuplicateName(String),
    DbError(String),
    CorruptFile { filename: String, reason: String },
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
            ImportError::CorruptFile { filename, reason } => {
                write!(
                    f,
                    "IMPORT_CORRUPT_FILE -- Failed to decode {}: {}",
                    filename, reason
                )
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

/// Register a project: scan files, decode DPX headers, check for duplicates, insert into DB.
/// Returns the project record and a variant breakdown for display.
pub fn register_project(
    conn: &Connection,
    name: &str,
    path: &Path,
) -> Result<(Project, VariantBreakdown), ImportError> {
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

    // Insert file records with DPX metadata extraction
    let mut variant_counts: HashMap<String, usize> = HashMap::new();
    let mut breakdown = VariantBreakdown::default();

    for (filename, format) in &files {
        let file_path = path.join(filename);

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
            "INSERT INTO files (project_id, filename, format, width, height, bit_depth, transfer, colorimetric, descriptor, compression, codec, fps, duration, frame_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![project_id, filename, format, width, height, bit_depth, transfer, colorimetric, descriptor, compression, codec, fps, duration, frame_count],
        )
        .map_err(|e| ImportError::DbError(e.to_string()))?;
    }

    // Build variant breakdown string
    let mut sorted_variants: Vec<_> = variant_counts.into_iter().collect();
    sorted_variants.sort_by(|a, b| b.1.cmp(&a.1));
    for (variant, count) in sorted_variants {
        breakdown.variants.push(format!("{}x {}", count, variant));
    }

    Ok((
        Project {
            id: project_id,
            name: name.to_string(),
            path: path.to_string_lossy().to_string(),
            dpx_count,
            exr_count,
            mov_count,
            created_at: 0,
        },
        breakdown,
    ))
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
    use mengxi_format::dpx::DpxEndian;
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
                width       INTEGER,
                height      INTEGER,
                bit_depth   INTEGER,
                transfer    TEXT,
                colorimetric TEXT,
                descriptor  TEXT,
                compression TEXT,
                codec       TEXT,
                fps         REAL,
                duration    REAL,
                frame_count INTEGER,
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
        let (project, _breakdown) = register_project(&conn, "my_film", &film_dir).unwrap();

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
        assert_eq!(files.len(), 7);
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

    #[test]
    fn test_register_with_dpx_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        fs::create_dir_all(&film_dir).unwrap();

        // Create a valid synthetic DPX file
        let dpx_path = film_dir.join("shot001.dpx");
        dpx::create_synthetic_dpx(&dpx_path, 1920, 1080, 10, 2, DpxEndian::Big).unwrap();

        let (_db_dir, conn) = setup_test_db();
        let (project, breakdown) = register_project(&conn, "meta_test", &film_dir).unwrap();

        assert_eq!(project.dpx_count, 1);
        assert_eq!(breakdown.variants.len(), 1);
        assert!(breakdown.variants[0].contains("10-bit"));
        assert!(breakdown.variants[0].contains("linear"));
        assert_eq!(breakdown.skipped_count, 0);
    }

    #[test]
    fn test_register_skips_corrupt_dpx() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        fs::create_dir_all(&film_dir).unwrap();

        // Create a valid DPX
        let valid_path = film_dir.join("valid.dpx");
        dpx::create_synthetic_dpx(&valid_path, 1920, 1080, 10, 2, DpxEndian::Big).unwrap();

        // Create a corrupt DPX (invalid magic, 2048 bytes)
        let corrupt_path = film_dir.join("corrupt.dpx");
        let mut data = vec![0u8; 2048];
        data[0] = b'B'; data[1] = b'A'; data[2] = b'D'; data[3] = b'!';
        fs::write(&corrupt_path, &data).unwrap();

        let (_db_dir, conn) = setup_test_db();
        let (project, breakdown) = register_project(&conn, "corrupt_test", &film_dir).unwrap();

        assert_eq!(project.dpx_count, 2); // Both files are .dpx
        assert_eq!(breakdown.skipped_count, 1);
        assert_eq!(breakdown.skipped_files.len(), 1);
        assert_eq!(breakdown.skipped_files[0], "corrupt.dpx");
        assert_eq!(breakdown.variants.len(), 1);
        assert!(breakdown.variants[0].contains("10-bit"));
    }

    #[test]
    fn test_register_with_mixed_bit_depths() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        fs::create_dir_all(&film_dir).unwrap();

        // Create two 10-bit DPX files
        for i in 0..2 {
            let p = film_dir.join(format!("shot{}.dpx", i));
            dpx::create_synthetic_dpx(&p, 1920, 1080, 10, 2, DpxEndian::Big).unwrap();
        }
        // Create one 16-bit DPX file
        let p16 = film_dir.join("shot16.dpx");
        dpx::create_synthetic_dpx(&p16, 4096, 2160, 16, 2, DpxEndian::Big).unwrap();
        // Create one valid EXR file
        let exr_path = film_dir.join("ref.exr");
        exr_format::create_synthetic_exr(&exr_path, 1920, 1080, exr::image::Encoding::UNCOMPRESSED).unwrap();

        let (_db_dir, conn) = setup_test_db();
        let (project, breakdown) = register_project(&conn, "mixed_test", &film_dir).unwrap();

        assert_eq!(project.dpx_count, 3);
        assert_eq!(project.exr_count, 1);
        assert_eq!(breakdown.skipped_count, 0);
        // DPX variants + EXR variant
        assert!(breakdown.variants.len() >= 2);

        let all_variants = breakdown.variants.join(", ");
        assert!(all_variants.contains("2x 10-bit"));
        assert!(all_variants.contains("1x 16-bit"));
        assert!(all_variants.contains("half-float NONE"));
    }

    #[test]
    fn test_register_with_exr_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        fs::create_dir_all(&film_dir).unwrap();

        // Create a valid EXR file
        let exr_path = film_dir.join("shot001.exr");
        exr_format::create_synthetic_exr(&exr_path, 1920, 1080, exr::image::Encoding::UNCOMPRESSED).unwrap();

        let (_db_dir, conn) = setup_test_db();
        let (project, breakdown) = register_project(&conn, "exr_meta_test", &film_dir).unwrap();

        assert_eq!(project.exr_count, 1);
        assert_eq!(breakdown.variants.len(), 1);
        assert!(breakdown.variants[0].contains("half-float"));
        assert!(breakdown.variants[0].contains("NONE"));
        assert_eq!(breakdown.skipped_count, 0);
    }

    #[test]
    fn test_register_skips_corrupt_exr() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        fs::create_dir_all(&film_dir).unwrap();

        // Create a valid EXR
        let valid_path = film_dir.join("valid.exr");
        exr_format::create_synthetic_exr(&valid_path, 1920, 1080, exr::image::Encoding::UNCOMPRESSED).unwrap();

        // Create a corrupt EXR (garbage bytes)
        let corrupt_path = film_dir.join("corrupt.exr");
        fs::write(&corrupt_path, vec![0xAB; 2048]).unwrap();

        let (_db_dir, conn) = setup_test_db();
        let (project, breakdown) = register_project(&conn, "corrupt_exr_test", &film_dir).unwrap();

        assert_eq!(project.exr_count, 2); // Both files are .exr
        assert_eq!(breakdown.skipped_count, 1);
        assert_eq!(breakdown.skipped_files.len(), 1);
        assert_eq!(breakdown.skipped_files[0], "corrupt.exr");
        assert_eq!(breakdown.variants.len(), 1);
        assert!(breakdown.variants[0].contains("half-float NONE"));
    }

    #[test]
    fn test_register_with_exr_compression_variants() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        fs::create_dir_all(&film_dir).unwrap();

        // Create 3 uncompressed EXR files
        for i in 0..3 {
            let p = film_dir.join(format!("shot{}.exr", i));
            exr_format::create_synthetic_exr(&p, 1920, 1080, exr::image::Encoding::UNCOMPRESSED).unwrap();
        }
        // Create 2 PIZ-compressed EXR files
        for i in 0..2 {
            let p = film_dir.join(format!("comp{}.exr", i));
            exr_format::create_synthetic_exr(&p, 1920, 1080, exr::image::Encoding::SMALL_FAST_LOSSLESS).unwrap();
        }

        let (_db_dir, conn) = setup_test_db();
        let (project, breakdown) = register_project(&conn, "exr_compress_test", &film_dir).unwrap();

        assert_eq!(project.exr_count, 5);
        assert_eq!(breakdown.skipped_count, 0);
        assert_eq!(breakdown.variants.len(), 2);

        let all_variants = breakdown.variants.join(", ");
        assert!(all_variants.contains("3x half-float NONE"));
        assert!(all_variants.contains("2x half-float PIZ"));
    }

    #[test]
    fn test_register_with_mixed_dpx_and_exr() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        fs::create_dir_all(&film_dir).unwrap();

        // Create 2 DPX files
        for i in 0..2 {
            let p = film_dir.join(format!("shot{}.dpx", i));
            dpx::create_synthetic_dpx(&p, 1920, 1080, 10, 2, DpxEndian::Big).unwrap();
        }
        // Create 3 EXR files with different compression
        exr_format::create_synthetic_exr(&film_dir.join("a.exr"), 1920, 1080, exr::image::Encoding::UNCOMPRESSED).unwrap();
        exr_format::create_synthetic_exr(&film_dir.join("b.exr"), 1920, 1080, exr::image::Encoding::FAST_LOSSLESS).unwrap();
        exr_format::create_synthetic_exr(&film_dir.join("c.exr"), 1920, 1080, exr::image::Encoding::SMALL_FAST_LOSSLESS).unwrap();

        let (_db_dir, conn) = setup_test_db();
        let (project, breakdown) = register_project(&conn, "mixed_dpx_exr", &film_dir).unwrap();

        assert_eq!(project.dpx_count, 2);
        assert_eq!(project.exr_count, 3);
        assert_eq!(breakdown.skipped_count, 0);
        // Should have 3 EXR variants + 1 DPX variant
        assert!(breakdown.variants.len() >= 3);

        let all_variants = breakdown.variants.join(", ");
        assert!(all_variants.contains("10-bit linear"));
        assert!(all_variants.contains("half-float NONE"));
        assert!(all_variants.contains("half-float RLE"));
        assert!(all_variants.contains("half-float PIZ"));
    }

    #[test]
    fn test_register_skips_corrupt_mov() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        fs::create_dir_all(&film_dir).unwrap();

        // Create a corrupt MOV (garbage bytes)
        let corrupt_path = film_dir.join("corrupt.mov");
        fs::write(&corrupt_path, vec![0xAB; 2048]).unwrap();

        let (_db_dir, conn) = setup_test_db();
        let (project, breakdown) = register_project(&conn, "corrupt_mov_test", &film_dir).unwrap();

        assert_eq!(project.mov_count, 1);
        assert_eq!(breakdown.skipped_count, 1);
        assert_eq!(breakdown.skipped_files.len(), 1);
        assert_eq!(breakdown.skipped_files[0], "corrupt.mov");
    }

    #[test]
    fn test_register_skips_corrupt_mov_alongside_valid_dpx() {
        let dir = tempfile::tempdir().unwrap();
        let film_dir = dir.path().join("film");
        fs::create_dir_all(&film_dir).unwrap();

        // Create a valid DPX
        let valid_path = film_dir.join("valid.dpx");
        dpx::create_synthetic_dpx(&valid_path, 1920, 1080, 10, 2, DpxEndian::Big).unwrap();

        // Create a corrupt MOV
        let corrupt_path = film_dir.join("corrupt.mov");
        fs::write(&corrupt_path, vec![0xAB; 1024]).unwrap();

        // Create a truncated MOV (too short for valid container)
        let trunc_path = film_dir.join("trunc.mov");
        fs::write(&trunc_path, vec![0u8; 50]).unwrap();

        let (_db_dir, conn) = setup_test_db();
        let (project, breakdown) = register_project(&conn, "mixed_corrupt_test", &film_dir).unwrap();

        assert_eq!(project.dpx_count, 1);
        assert_eq!(project.mov_count, 2);
        assert_eq!(breakdown.skipped_count, 2);
        assert_eq!(breakdown.skipped_files.len(), 2);
        assert!(breakdown.skipped_files.contains(&"corrupt.mov".to_string()));
        assert!(breakdown.skipped_files.contains(&"trunc.mov".to_string()));
        // DPX variant should still be present
        assert!(breakdown.variants.iter().any(|v| v.contains("10-bit")));
    }
}
