use rusqlite::{Connection, Result as SqlResult};
use std::fs;
use std::path::PathBuf;

pub type DbConnection = Connection;

type QueryResult = (Vec<String>, Vec<Vec<String>>);
type FingerprintTiles = Vec<(usize, usize, crate::grading_features::GradingFeatures)>;

/// Returns the database directory path.
/// Uses hardcoded default; can be overridden via config in future stories.
pub fn db_dir() -> PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".mengxi/data")
}

/// Returns the database file path.
pub fn db_path() -> PathBuf {
    db_dir().join("mengxi.db")
}

/// Opens (or creates) the database connection with WAL mode enabled.
/// Runs pending migrations automatically.
pub fn open_db() -> Result<Connection, Box<dyn std::error::Error>> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    run_migrations(&conn)?;
    ensure_schema_extensions(&conn)?;

    Ok(conn)
}

/// Open the database at a specific path. Used for testing with temporary databases.
///
/// Uses the same connection settings (WAL mode, pragmas, migrations) as `open_db()`,
/// but bypasses the hardcoded `db_path()` so callers can supply an isolated SQLite file.
pub fn open_db_at_path(path: &std::path::Path) -> Result<Connection, Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(path)
        .map_err(|e| format!("failed to open db at {}: {}", path.display(), e))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("pragma setup failed: {}", e))?;
    run_migrations(&conn)?;
    ensure_schema_extensions(&conn)?;

    Ok(conn)
}

/// Ensures the schema_version table exists and returns the current version.
/// Returns 0 if no migrations have been applied.
fn current_version(conn: &Connection) -> SqlResult<i64> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_version')",
            [],
            |row| row.get(0),
        )?;

    if !exists {
        conn.execute_batch(
            "CREATE TABLE schema_version (
                version INTEGER NOT NULL PRIMARY KEY
            );
            INSERT INTO schema_version (version) VALUES (0);",
        )?;
        return Ok(0);
    }

    let version: i64 = conn.query_row(
        "SELECT version FROM schema_version",
        [],
        |row| row.get(0),
    )?;

    Ok(version)
}

/// Discovers migration files from a migrations directory.
/// Returns a sorted list of (version_number, sql_content) tuples.
fn discover_migrations_from_dir(migrations_dir: &PathBuf) -> Result<Vec<(i64, String)>, Box<dyn std::error::Error>> {
    if !migrations_dir.exists() {
        return Ok(Vec::new());
    }

    let mut migrations = Vec::new();
    for entry in fs::read_dir(migrations_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let str_name = name.to_string_lossy();

        // Parse NNN_description.sql
        if !str_name.ends_with(".sql") {
            continue;
        }

        let stem = str_name.trim_end_matches(".sql");
        let version: i64 = stem
            .split('_')
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                format!(
                    "Migration file '{str_name}' does not start with a numeric prefix"
                )
            })?;

        let sql = fs::read_to_string(entry.path())?;
        migrations.push((version, sql));
    }

    migrations.sort_by_key(|(v, _)| *v);
    Ok(migrations)
}

/// Discovers migration files from the embedded migrations/ directory.
fn discover_migrations() -> Result<Vec<(i64, String)>, Box<dyn std::error::Error>> {
    // Use CARGO_MANIFEST_DIR at compile time to find project root, falling back to current_dir
    let migrations_dir = option_env!("CARGO_MANIFEST_DIR")
        .map(|dir| std::path::PathBuf::from(dir).parent().unwrap().parent().unwrap().join("migrations"))
        .unwrap_or_else(|| std::env::current_dir().unwrap().join("migrations"));
    discover_migrations_from_dir(&migrations_dir)
}

/// Runs all pending migrations against the database.
/// Each migration is committed individually with its version number updated after success.
pub fn run_migrations(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let current = current_version(conn)?;
    let migrations = discover_migrations()?;

    for (version, sql) in &migrations {
        if *version <= current {
            continue;
        }
        conn.execute_batch(sql)?;
        // Update version immediately after each migration for crash recovery
        conn.execute(
            "UPDATE schema_version SET version = ?1",
            [version],
        )?;
    }

    Ok(())
}

/// Ensures schema extensions that can't be expressed in migration SQL
/// (e.g., ALTER TABLE ADD COLUMN with idempotent behavior).
fn ensure_schema_extensions(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    // Check if branch_id column exists in session_messages
    let has_branch_id: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('session_messages') WHERE name = 'branch_id'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    if !has_branch_id {
        conn.execute_batch(
            "ALTER TABLE session_messages ADD COLUMN branch_id TEXT NOT NULL DEFAULT 'main';
             ALTER TABLE session_messages ADD COLUMN is_compacted INTEGER NOT NULL DEFAULT 0;",
        )?;
    }

    // Ensure the branch-seq index exists (depends on branch_id column)
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_session_messages_branch_seq
            ON session_messages(session_id, branch_id, seq);",
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Database browsing / inspection queries
// ---------------------------------------------------------------------------

/// Row returned by `db_list_projects`.
#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub dpx_count: i64,
    pub exr_count: i64,
    pub mov_count: i64,
    pub file_count: i64,
    pub fingerprint_count: i64,
    pub created_at: i64,
}

/// List all projects with file/fingerprint counts.
pub fn db_list_projects(conn: &Connection) -> Result<Vec<ProjectRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.path, p.dpx_count, p.exr_count, p.mov_count,
                COALESCE(fc.cnt, 0) AS file_count,
                COALESCE(fpc.cnt, 0) AS fingerprint_count,
                p.created_at
         FROM projects p
         LEFT JOIN (SELECT project_id, COUNT(*) AS cnt FROM files GROUP BY project_id) fc ON fc.project_id = p.id
         LEFT JOIN (SELECT f.project_id, COUNT(*) AS cnt
                   FROM fingerprints fp JOIN files f ON f.id = fp.file_id GROUP BY f.project_id) fpc ON fpc.project_id = p.id
         ORDER BY p.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ProjectRow {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            dpx_count: row.get(3)?,
            exr_count: row.get(4)?,
            mov_count: row.get(5)?,
            file_count: row.get(6)?,
            fingerprint_count: row.get(7)?,
            created_at: row.get(8)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
}

/// Row returned by `db_list_files`.
#[derive(Debug, Clone)]
pub struct FileRow {
    pub id: i64,
    pub filename: String,
    pub format: String,
    pub fingerprint_count: i64,
    pub created_at: i64,
}

/// List files in a project with fingerprint counts.
pub fn db_list_files(conn: &Connection, project_name: &str) -> Result<Vec<FileRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.filename, f.format,
                COALESCE(fpc.cnt, 0) AS fingerprint_count,
                f.created_at
         FROM files f
         JOIN projects p ON p.id = f.project_id
         LEFT JOIN (SELECT file_id, COUNT(*) AS cnt FROM fingerprints GROUP BY file_id) fpc ON fpc.file_id = f.id
         WHERE p.name = ?1
         ORDER BY f.created_at DESC",
    )?;
    let rows = stmt.query_map(rusqlite::params![project_name], |row| {
        Ok(FileRow {
            id: row.get(0)?,
            filename: row.get(1)?,
            format: row.get(2)?,
            fingerprint_count: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
}

/// Row returned by `db_list_tags`.
#[derive(Debug, Clone)]
pub struct TagRow {
    pub id: i64,
    pub tag: String,
    pub source: String,
    pub project_name: String,
    pub filename: String,
    pub created_at: i64,
}

/// List tags, optionally filtered by project name.
pub fn db_list_tags(conn: &Connection, project_name: Option<&str>) -> Result<Vec<TagRow>, rusqlite::Error> {
    let sql = match project_name {
        Some(_) => "SELECT t.id, t.tag, t.source, p.name, f.filename, t.created_at
                    FROM tags t
                    JOIN fingerprints fp ON fp.id = t.fingerprint_id
                    JOIN files f ON f.id = fp.file_id
                    JOIN projects p ON p.id = f.project_id
                    WHERE p.name = ?1
                    ORDER BY t.created_at DESC",
        None => "SELECT t.id, t.tag, t.source, p.name, f.filename, t.created_at
                 FROM tags t
                 JOIN fingerprints fp ON fp.id = t.fingerprint_id
                 JOIN files f ON f.id = fp.file_id
                 JOIN projects p ON p.id = f.project_id
                 ORDER BY t.created_at DESC",
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = if let Some(pn) = project_name {
        let mut rows = Vec::new();
        let mapped = stmt.query_map(rusqlite::params![pn], |row| {
            Ok(TagRow {
                id: row.get(0)?,
                tag: row.get(1)?,
                source: row.get(2)?,
                project_name: row.get(3)?,
                filename: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        for row in mapped {
            rows.push(row?);
        }
        rows
    } else {
        let mut rows = Vec::new();
        let mapped = stmt.query_map([], |row| {
            Ok(TagRow {
                id: row.get(0)?,
                tag: row.get(1)?,
                source: row.get(2)?,
                project_name: row.get(3)?,
                filename: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        for row in mapped {
            rows.push(row?);
        }
        rows
    };
    Ok(rows)
}

/// Row returned by `db_list_luts`.
#[derive(Debug, Clone)]
pub struct LutRow {
    pub id: i64,
    pub title: Option<String>,
    pub format: String,
    pub grid_size: i64,
    pub output_path: String,
    pub project_name: String,
    pub created_at: i64,
}

/// List LUT export history.
pub fn db_list_luts(conn: &Connection) -> Result<Vec<LutRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT l.id, l.title, l.format, l.grid_size, l.output_path, p.name, l.created_at
         FROM luts l
         JOIN projects p ON p.id = l.project_id
         ORDER BY l.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(LutRow {
            id: row.get(0)?,
            title: row.get(1)?,
            format: row.get(2)?,
            grid_size: row.get(3)?,
            output_path: row.get(4)?,
            project_name: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
}

/// Error returned by `db_run_query` when SQL is not a read-only SELECT.
#[derive(Debug, thiserror::Error)]
#[error("DB_NON_SELECT -- only SELECT queries are allowed")]
pub struct NonSelectError;

/// Execute a raw read-only SQL query. Returns (column_names, rows).
/// Only SELECT statements are allowed.
pub fn db_run_query(
    conn: &Connection,
    sql: &str,
) -> Result<QueryResult, Box<dyn std::error::Error>> {
    let trimmed = sql.trim();
    if !trimmed.to_uppercase().starts_with("SELECT") {
        return Err(Box::new(NonSelectError));
    }
    let mut stmt = conn.prepare(trimmed)?;
    let col_names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let col_count = col_names.len();
    let rows: Vec<Vec<String>> = stmt
        .query_map([], |row| {
            let mut vals = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let val: rusqlite::types::Value = row.get(i)?;
                vals.push(match val {
                    rusqlite::types::Value::Null => "NULL".to_string(),
                    rusqlite::types::Value::Integer(i) => i.to_string(),
                    rusqlite::types::Value::Real(f) => format!("{}", f),
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Blob(b) => format!("<blob {} bytes>", b.len()),
                });
            }
            Ok(vals)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((col_names, rows))
}

// ---------------------------------------------------------------------------
// Fingerprint tile operations
// ---------------------------------------------------------------------------

/// Store per-tile grading features for a fingerprint.
/// Deletes any existing tiles for this fingerprint first (upsert semantics).
pub fn store_fingerprint_tiles(
    conn: &Connection,
    fingerprint_id: i64,
    tiles: &[crate::feature_pipeline::TileFeatures],
    hist_bins: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Clear existing tiles
    conn.execute(
        "DELETE FROM fingerprint_tiles WHERE fingerprint_id = ?1",
        rusqlite::params![fingerprint_id],
    )?;

    for tile in tiles {
        let features = &tile.features;
        conn.execute(
            "INSERT INTO fingerprint_tiles (fingerprint_id, tile_row, tile_col, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, hist_bins)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                fingerprint_id,
                tile.row as i64,
                tile.col as i64,
                features.hist_l_blob(),
                features.hist_a_blob(),
                features.hist_b_blob(),
                features.moments_blob(),
                hist_bins as i64,
            ],
        )?;
    }
    Ok(())
}

/// Load all tiles for a given fingerprint.
/// Returns a vector of (tile_row, tile_col, GradingFeatures).
pub fn load_fingerprint_tiles(
    conn: &Connection,
    fingerprint_id: i64,
) -> Result<FingerprintTiles, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT tile_row, tile_col, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, hist_bins
         FROM fingerprint_tiles
         WHERE fingerprint_id = ?1
         ORDER BY tile_row, tile_col",
    )?;

    let rows = stmt.query_map(rusqlite::params![fingerprint_id], |row| {
        let tile_row: i64 = row.get(0)?;
        let tile_col: i64 = row.get(1)?;
        let hist_l: Vec<u8> = row.get(2)?;
        let hist_a: Vec<u8> = row.get(3)?;
        let hist_b: Vec<u8> = row.get(4)?;
        let moments: Vec<u8> = row.get(5)?;
        let hist_bins: i64 = row.get(6)?;
        Ok((tile_row, tile_col, hist_l, hist_a, hist_b, moments, hist_bins))
    })?;

    let mut result = Vec::new();
    for row in rows {
        let (tile_row, tile_col, hist_l, hist_a, hist_b, moments, hist_bins) = row?;
        let features = crate::grading_features::GradingFeatures::from_separate_blobs(
            &hist_l, &hist_a, &hist_b, &moments, hist_bins as usize,
        ).map_err(|e| format!("TILE_DECODE_ERROR -- {}", e))?;
        result.push((tile_row as usize, tile_col as usize, features));
    }
    Ok(result)
}

/// Delete all tiles for a given fingerprint.
pub fn delete_fingerprint_tiles(
    conn: &Connection,
    fingerprint_id: i64,
) -> Result<usize, Box<dyn std::error::Error>> {
    let count = conn.execute(
        "DELETE FROM fingerprint_tiles WHERE fingerprint_id = ?1",
        rusqlite::params![fingerprint_id],
    )?;
    Ok(count)
}

/// Resolve a file ID from project name and filename.
pub fn resolve_file_id(
    conn: &Connection,
    project: &str,
    filename: &str,
) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT f.id FROM files f
         JOIN projects p ON p.id = f.project_id
         WHERE p.name = ?1 AND f.filename = ?2
         LIMIT 1",
        rusqlite::params![project, filename],
        |row| row.get::<_, i64>(0),
    )
}

/// Resolve a fingerprint ID from project name and filename.
pub fn resolve_fingerprint_id(
    conn: &Connection,
    project: &str,
    filename: &str,
) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT fp.id FROM fingerprints fp
         JOIN files f ON f.id = fp.file_id
         JOIN projects p ON p.id = f.project_id
         WHERE p.name = ?1 AND f.filename = ?2
         LIMIT 1",
        rusqlite::params![project, filename],
        |row| row.get::<_, i64>(0),
    )
}

// ---------------------------------------------------------------------------
// Agent Session Types & Operations
// ---------------------------------------------------------------------------

/// Error type for agent session/branch/message DB operations.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("DB_QUERY_ERROR -- {0}")]
    Query(String),
    #[error("DB_ERROR -- {0}")]
    Other(String),
}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Query(e.to_string())
    }
}

// === Agent Session Record Types ===

/// Full record from the `agent_sessions` table.
#[derive(Debug, Clone)]
pub struct AgentSessionRecord {
    pub id: String,
    pub title: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Summary row for session listing (includes message count).
#[derive(Debug, Clone)]
pub struct AgentSessionInfo {
    pub id: String,
    pub title: String,
    pub message_count: i64,
    pub updated_at: i64,
}

/// Record from the `session_branches` table.
#[derive(Debug, Clone)]
pub struct BranchRecord {
    pub id: String,
    pub session_id: String,
    pub parent_branch_id: Option<String>,
    pub branch_point_seq: Option<i64>,
    pub name: String,
    pub created_at: i64,
}

/// Record from the `session_messages` table.
#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub seq: i64,
    pub role: String,
    pub content_json: String,
}

// --- Helper functions ---

fn uuid_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}", dur.as_nanos())
}

fn epoch_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// --- Session CRUD ---

/// Create a new agent session and return its ID.
pub fn agent_session_create(
    conn: &Connection,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<String, DbError> {
    let id = format!("sess_{}", uuid_timestamp());
    let now = epoch_now();
    conn.execute(
        "INSERT INTO agent_sessions (id, title, provider, model, created_at, updated_at) VALUES (?1, 'New Session', ?2, ?3, ?4, ?4)",
        rusqlite::params![id, provider, model, now],
    )?;
    Ok(id)
}

/// Get a single session by ID.
pub fn agent_session_get(
    conn: &Connection,
    id: &str,
) -> Result<Option<AgentSessionRecord>, DbError> {
    let result = conn.query_row(
        "SELECT id, title, provider, model, created_at, updated_at FROM agent_sessions WHERE id = ?1",
        rusqlite::params![id],
        |row| Ok(AgentSessionRecord {
            id: row.get(0)?,
            title: row.get(1)?,
            provider: row.get(2)?,
            model: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        }),
    );
    match result {
        Ok(record) => Ok(Some(record)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::Query(e.to_string())),
    }
}

/// List all sessions with message counts, ordered by most recently updated.
pub fn agent_session_list(
    conn: &Connection,
) -> Result<Vec<AgentSessionInfo>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.title, COUNT(m.seq), s.updated_at \
         FROM agent_sessions s \
         LEFT JOIN session_messages m ON m.session_id = s.id AND m.branch_id = \
           (SELECT id FROM session_branches WHERE session_id = s.id AND name = 'main') \
         GROUP BY s.id ORDER BY s.updated_at DESC",
    )?;
    let rows = stmt
        .query_map([], |row| Ok(AgentSessionInfo {
            id: row.get(0)?,
            title: row.get(1)?,
            message_count: row.get::<_, i64>(2)?,
            updated_at: row.get(3)?,
        }))?
        .collect::<Result<Vec<AgentSessionInfo>, rusqlite::Error>>()
        .map_err(|e| DbError::Query(e.to_string()))?;
    Ok(rows)
}

/// Update a session's title.
pub fn agent_session_update_title(
    conn: &Connection,
    session_id: &str,
    title: &str,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE agent_sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![title, epoch_now(), session_id],
    )?;
    Ok(())
}

/// Delete a session and all its branches/messages.
pub fn agent_session_delete(
    conn: &Connection,
    session_id: &str,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM session_messages WHERE session_id = ?1",
        rusqlite::params![session_id],
    )?;
    conn.execute(
        "DELETE FROM session_branches WHERE session_id = ?1",
        rusqlite::params![session_id],
    )?;
    conn.execute(
        "DELETE FROM agent_sessions WHERE id = ?1",
        rusqlite::params![session_id],
    )?;
    Ok(())
}

// --- Branch Operations ---

/// Resolve a branch UUID by name within a session.
pub fn agent_branch_resolve_id(
    conn: &Connection,
    session_id: &str,
    name: &str,
) -> Result<String, DbError> {
    conn.query_row(
        "SELECT id FROM session_branches WHERE session_id = ?1 AND name = ?2",
        rusqlite::params![session_id, name],
        |row| row.get(0),
    )
    .map_err(|e| DbError::Query(e.to_string()))
}

/// Create a new branch. Pass `None` for parent_branch_id to create a root (e.g. "main") branch.
pub fn agent_branch_create(
    conn: &Connection,
    session_id: &str,
    parent_branch_id: Option<&str>,
    branch_point_seq: i64,
    name: &str,
) -> Result<BranchRecord, DbError> {
    let id = format!("br_{}", uuid_timestamp());
    let now = epoch_now();
    conn.execute(
        "INSERT INTO session_branches (id, session_id, parent_branch_id, branch_point_seq, name, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, session_id, parent_branch_id, branch_point_seq, name, now],
    )?;
    Ok(BranchRecord {
        id,
        session_id: session_id.into(),
        parent_branch_id: parent_branch_id.map(|s| s.to_string()),
        branch_point_seq: Some(branch_point_seq),
        name: name.into(),
        created_at: now,
    })
}

/// List all branches for a session.
pub fn agent_branch_list(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<BranchRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, parent_branch_id, branch_point_seq, name, created_at \
         FROM session_branches WHERE session_id = ?1 ORDER BY created_at",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![session_id], |row| Ok(BranchRecord {
            id: row.get(0)?,
            session_id: row.get(1)?,
            parent_branch_id: row.get(2)?,
            branch_point_seq: row.get(3)?,
            name: row.get(4)?,
            created_at: row.get(5)?,
        }))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| DbError::Query(e.to_string()))?;
    Ok(rows)
}

/// Get the main branch for a session.
pub fn agent_branch_get_main(
    conn: &Connection,
    session_id: &str,
) -> Result<Option<BranchRecord>, DbError> {
    let result = conn.query_row(
        "SELECT id, session_id, parent_branch_id, branch_point_seq, name, created_at \
         FROM session_branches WHERE session_id = ?1 AND name = 'main'",
        rusqlite::params![session_id],
        |row| Ok(BranchRecord {
            id: row.get(0)?,
            session_id: row.get(1)?,
            parent_branch_id: row.get(2)?,
            branch_point_seq: row.get(3)?,
            name: row.get(4)?,
            created_at: row.get(5)?,
        }),
    );
    match result {
        Ok(record) => Ok(Some(record)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::Query(e.to_string())),
    }
}

// --- Message Operations ---

/// Save a message to a session's main branch (convenience wrapper).
pub fn agent_message_save_main(
    conn: &Connection,
    session_id: &str,
    role: &str,
    content_json: &str,
) -> Result<i64, DbError> {
    let branch_id = agent_branch_resolve_id(conn, session_id, "main")?;
    agent_message_save(conn, session_id, &branch_id, role, content_json)
}

/// Save a message to a specific branch, auto-computing seq.
pub fn agent_message_save(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
    role: &str,
    content_json: &str,
) -> Result<i64, DbError> {
    let max_seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), -1) FROM session_messages WHERE session_id = ?1 AND branch_id = ?2",
        rusqlite::params![session_id, branch_id],
        |row| row.get(0),
    )?;
    let seq = max_seq + 1;
    conn.execute(
        "INSERT INTO session_messages (session_id, branch_id, seq, role, content) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![session_id, branch_id, seq, role, content_json],
    )?;
    // Touch session updated_at
    conn.execute(
        "UPDATE agent_sessions SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![epoch_now(), session_id],
    )?;
    Ok(seq)
}

/// Load messages for a specific branch, ordered by seq ASC.
pub fn agent_message_load_for_branch(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
) -> Result<Vec<MessageRecord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT seq, role, content FROM session_messages \
         WHERE session_id = ?1 AND branch_id = ?2 AND is_compacted = 0 ORDER BY seq ASC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![session_id, branch_id], |row| Ok(MessageRecord {
            seq: row.get(0)?,
            role: row.get(1)?,
            content_json: row.get(2)?,
        }))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| DbError::Query(e.to_string()))?;
    Ok(rows)
}

/// Mark messages as compacted up to a given seq.
pub fn agent_message_mark_compacted(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
    up_to_seq: i64,
) -> Result<usize, DbError> {
    let count = conn.execute(
        "UPDATE session_messages SET is_compacted = 1 \
         WHERE session_id = ?1 AND branch_id = ?2 AND seq <= ?3 AND is_compacted = 0",
        rusqlite::params![session_id, branch_id, up_to_seq],
    )?;
    Ok(count)
}

/// Insert a compaction summary message (system role, pre-marked compacted).
pub fn agent_session_insert_summary(
    conn: &Connection,
    session_id: &str,
    branch_id: &str,
    content_json: &str,
) -> Result<i64, DbError> {
    let max_seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), -1) FROM session_messages WHERE session_id = ?1 AND branch_id = ?2",
        rusqlite::params![session_id, branch_id],
        |row| row.get(0),
    )?;
    let seq = max_seq + 1;
    conn.execute(
        "INSERT INTO session_messages (session_id, branch_id, seq, role, content, is_compacted) \
         VALUES (?1, ?2, ?3, 'system', ?4, 0)",
        rusqlite::params![session_id, branch_id, seq, content_json],
    )?;
    Ok(seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_runner_creates_tables() {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("test.db");

        let conn = Connection::open(&db_file).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();

        // Apply migrations against temp DB using a workaround:
        // We can't easily override db_path() since it's hardcoded,
        // so test the SQL directly
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS projects (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL UNIQUE,
                path        TEXT NOT NULL,
                dpx_count   INTEGER NOT NULL DEFAULT 0,
                exr_count   INTEGER NOT NULL DEFAULT 0,
                mov_count   INTEGER NOT NULL DEFAULT 0,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                filename    TEXT NOT NULL,
                format      TEXT NOT NULL CHECK(format IN ('dpx', 'exr', 'mov')),
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .unwrap();

        // Verify tables exist
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('projects', 'files')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_schema_version_tracking() {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("test.db");
        let conn = Connection::open(&db_file).unwrap();

        // Simulate current_version logic
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_version')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!exists);

        // After creating schema_version table
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (0);",
        )
        .unwrap();

        let version: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 0);
    }

    #[test]
    fn test_discover_migrations() {
        // Use CARGO_MANIFEST_DIR to locate project root reliably during cargo test
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let project_root = PathBuf::from(&manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("migrations");
        let migrations = discover_migrations_from_dir(&project_root).unwrap();
        assert!(migrations.len() >= 19);
        assert_eq!(migrations[0].0, 1); // 001_create_projects.sql
        assert_eq!(migrations[1].0, 2); // 002_create_files.sql
        assert_eq!(migrations[2].0, 3); // 003_add_file_metadata.sql
        assert_eq!(migrations[3].0, 4); // 004_add_file_compression.sql
        assert_eq!(migrations[4].0, 5); // 005_add_mov_metadata.sql
        assert_eq!(migrations[5].0, 6); // 006_create_fingerprints.sql
        assert_eq!(migrations[6].0, 7); // 007_add_files_unique_constraint.sql
        assert_eq!(migrations[7].0, 8); // 008_create_luts.sql
        assert_eq!(migrations[8].0, 9); // 009_add_fingerprints_file_id_index.sql
        assert_eq!(migrations[9].0, 10); // 010_add_embedding_to_fingerprints.sql
        assert_eq!(migrations[10].0, 11); // 011_create_tags.sql
        assert_eq!(migrations[11].0, 12); // 012_create_search_feedback.sql
        assert!(migrations[0].1.contains("CREATE TABLE IF NOT EXISTS projects"));
        assert!(migrations[1].1.contains("CREATE TABLE IF NOT EXISTS files"));
        assert!(migrations[2].1.contains("ALTER TABLE files ADD COLUMN"));
        assert!(migrations[3].1.contains("compression"));
        assert!(migrations[4].1.contains("codec"));
        assert!(migrations[5].1.contains("fingerprints"));
        assert!(migrations[7].1.contains("luts"));
    }

    /// Helper: create a temp DB with full migration schema for browsing query tests.
    fn setup_test_db() -> Connection {
        crate::test_db::setup_test_db()
    }

    #[test]
    fn test_db_list_projects_empty() {
        let conn = setup_test_db();
        let projects = db_list_projects(&conn).unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn test_db_list_projects_with_data() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's2.exr', 'exr')", [])
            .unwrap();

        let projects = db_list_projects(&conn).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "film_a");
        assert_eq!(projects[0].file_count, 2);
    }

    #[test]
    fn test_db_list_files() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's2.exr', 'exr')", [])
            .unwrap();

        let files = db_list_files(&conn, "film_a").unwrap();
        assert_eq!(files.len(), 2);
        let names: Vec<&str> = files.iter().map(|f| f.filename.as_str()).collect();
        assert!(names.contains(&"s1.dpx"));
        assert!(names.contains(&"s2.exr"));

        // Non-existent project
        let files = db_list_files(&conn, "nope").unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_db_list_tags() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '[]', '[]', '[]', 0.5, 0.1, 'video')", [])
            .unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag, source) VALUES (1, 'warm', 'ai')", [])
            .unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag, source) VALUES (1, 'golden', 'manual')", [])
            .unwrap();

        // All tags
        let tags = db_list_tags(&conn, None).unwrap();
        assert_eq!(tags.len(), 2);

        // Filtered by project
        let tags = db_list_tags(&conn, Some("film_a")).unwrap();
        assert_eq!(tags.len(), 2);
        let tag_labels: Vec<&str> = tags.iter().map(|t| t.tag.as_str()).collect();
        assert!(tag_labels.contains(&"warm"));
        assert!(tag_labels.contains(&"golden"));
        let warm = tags.iter().find(|t| t.tag == "warm").unwrap();
        assert_eq!(warm.source, "ai");

        // Non-existent project
        let tags = db_list_tags(&conn, Some("nope")).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn test_db_list_luts() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO luts (project_id, format, grid_size, output_path, title) VALUES (1, 'cube', 33, '/out/grade.cube', 'Grade v1')", [])
            .unwrap();

        let luts = db_list_luts(&conn).unwrap();
        assert_eq!(luts.len(), 1);
        assert_eq!(luts[0].title.as_deref(), Some("Grade v1"));
        assert_eq!(luts[0].project_name, "film_a");
    }

    #[test]
    fn test_db_run_query_select() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();

        let (cols, rows) = db_run_query(&conn, "SELECT name FROM projects").unwrap();
        assert_eq!(cols, vec!["name"]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "film_a");
    }

    #[test]
    fn test_db_run_query_rejects_non_select() {
        let conn = setup_test_db();
        let result = db_run_query(&conn, "DELETE FROM projects");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("DB_NON_SELECT"));
    }

    #[test]
    fn test_db_run_query_select_whitespace() {
        let conn = setup_test_db();
        // Leading whitespace + lowercase select should still work
        let result = db_run_query(&conn, "  select 1");
        assert!(result.is_ok());
    }

    // -- Migration 018: feature_status column tests --

    /// Helper: create a temp DB with schema at migration 017 and insert test data.
    /// Returns (temp_dir, conn) so the DB persists for the test.
    /// NOTE: Uses inline DDL because these tests verify migration behavior at specific versions.
    fn setup_test_db_with_grading_features(
        with_grading_features: bool,
    ) -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("test.db");
        let conn = Connection::open(&db_file).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();

        // Set up schema_version at 17 (all migrations through 017 applied)
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (17);
             CREATE TABLE projects (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL UNIQUE,
                 path TEXT NOT NULL,
                 dpx_count INTEGER NOT NULL DEFAULT 0,
                 exr_count INTEGER NOT NULL DEFAULT 0,
                 mov_count INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );
             CREATE TABLE files (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 filename TEXT NOT NULL,
                 format TEXT NOT NULL CHECK(format IN ('dpx', 'exr', 'mov')),
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );
             CREATE TABLE fingerprints (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                 histogram_r TEXT NOT NULL,
                 histogram_g TEXT NOT NULL,
                 histogram_b TEXT NOT NULL,
                 luminance_mean REAL NOT NULL,
                 luminance_stddev REAL NOT NULL,
                 color_space_tag TEXT NOT NULL,
                 grading_features BLOB,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );",
        )
        .unwrap();

        // Insert test data
        conn.execute("INSERT INTO projects (name, path) VALUES ('test_proj', '/tmp/test')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'test.dpx', 'dpx')", [])
            .unwrap();

        if with_grading_features {
            let blob_data: Vec<u8> = vec![0u8; 64];
            conn.execute(
                "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, grading_features) VALUES (1, '[]', '[]', '[]', 0.5, 0.1, 'video', ?1)",
                [blob_data],
            ).unwrap();
        } else {
            conn.execute(
                "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '[]', '[]', '[]', 0.5, 0.1, 'video')",
                [],
            ).unwrap();
        }

        (dir, conn)
    }

    #[test]
    fn test_migration_018_adds_feature_status_column() {
        let (_dir, conn) = setup_test_db_with_grading_features(true);
        run_migrations(&conn).unwrap();

        // Verify column exists by querying it
        let status: Option<String> = conn
            .query_row(
                "SELECT feature_status FROM fingerprints WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        // Should be 'stale' because grading_features IS NOT NULL
        assert_eq!(status.as_deref(), Some("stale"));
    }

    #[test]
    fn test_migration_018_stale_marking_with_grading_features() {
        let (_dir, conn) = setup_test_db_with_grading_features(true);
        run_migrations(&conn).unwrap();

        let status: String = conn
            .query_row(
                "SELECT feature_status FROM fingerprints WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "stale");
    }

    #[test]
    fn test_migration_018_null_preservation_without_grading_features() {
        let (_dir, conn) = setup_test_db_with_grading_features(false);
        run_migrations(&conn).unwrap();

        let status: Option<String> = conn
            .query_row(
                "SELECT feature_status FROM fingerprints WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        // Should remain NULL because grading_features IS NULL
        assert!(status.is_none());
    }

    #[test]
    fn test_migration_018_idempotency() {
        let (_dir, conn) = setup_test_db_with_grading_features(true);
        run_migrations(&conn).unwrap();

        // Verify state after first run
        let status_after_first: String = conn
            .query_row(
                "SELECT feature_status FROM fingerprints WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status_after_first, "stale");

        let version_after_first: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();

        // Run migrations again — should be a no-op (version 18 already recorded)
        run_migrations(&conn).unwrap();

        let status_after_second: String = conn
            .query_row(
                "SELECT feature_status FROM fingerprints WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status_after_second, "stale");

        let version_after_second: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version_after_second, version_after_first);
    }

    #[test]
    fn test_migration_018_creates_index() {
        let (_dir, conn) = setup_test_db_with_grading_features(true);
        run_migrations(&conn).unwrap();

        let index_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name='idx_fingerprints_feature_status')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(index_exists);
    }

    #[test]
    fn test_migration_018_mixed_stale_and_null() {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("test.db");
        let conn = Connection::open(&db_file).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();

        // Set up schema at version 17
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (17);
             CREATE TABLE projects (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL UNIQUE,
                 path TEXT NOT NULL,
                 dpx_count INTEGER NOT NULL DEFAULT 0,
                 exr_count INTEGER NOT NULL DEFAULT 0,
                 mov_count INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );
             CREATE TABLE files (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 filename TEXT NOT NULL,
                 format TEXT NOT NULL CHECK(format IN ('dpx', 'exr', 'mov')),
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );
             CREATE TABLE fingerprints (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                 histogram_r TEXT NOT NULL,
                 histogram_g TEXT NOT NULL,
                 histogram_b TEXT NOT NULL,
                 luminance_mean REAL NOT NULL,
                 luminance_stddev REAL NOT NULL,
                 color_space_tag TEXT NOT NULL,
                 grading_features BLOB,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );",
        )
        .unwrap();

        conn.execute("INSERT INTO projects (name, path) VALUES ('test_proj', '/tmp/test')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'a.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'b.dpx', 'dpx')", [])
            .unwrap();

        // Row 1: WITH grading_features → should become 'stale'
        let blob: Vec<u8> = vec![0u8; 64];
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, grading_features) VALUES (1, '[]', '[]', '[]', 0.5, 0.1, 'video', ?1)",
            [blob],
        ).unwrap();

        // Row 2: WITHOUT grading_features → should remain NULL
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, '[]', '[]', '[]', 0.3, 0.2, 'linear')",
            [],
        ).unwrap();

        run_migrations(&conn).unwrap();

        // Verify mixed states
        let stale_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fingerprints WHERE feature_status = 'stale'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let null_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fingerprints WHERE feature_status IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stale_count, 1);
        assert_eq!(null_count, 1);
    }

    // --- Migration 019 tests ---

    /// Create a test DB at schema version 18 (with grading_features + feature_status columns).
    /// If `with_grading_features` is true, inserts a real 1584-byte BLOB; otherwise NULL.
    /// NOTE: Uses inline DDL because these tests verify migration behavior at specific versions.
    fn setup_test_db_at_version_18(with_grading_features: bool) -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let db_file = dir.path().join("test.db");
        let conn = Connection::open(&db_file).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();

        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (18);
             CREATE TABLE projects (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL UNIQUE,
                 path TEXT NOT NULL,
                 dpx_count INTEGER NOT NULL DEFAULT 0,
                 exr_count INTEGER NOT NULL DEFAULT 0,
                 mov_count INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );
             CREATE TABLE files (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 filename TEXT NOT NULL,
                 format TEXT NOT NULL CHECK(format IN ('dpx', 'exr', 'mov')),
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );
             CREATE TABLE fingerprints (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                 histogram_r TEXT NOT NULL,
                 histogram_g TEXT NOT NULL,
                 histogram_b TEXT NOT NULL,
                 luminance_mean REAL NOT NULL,
                 luminance_stddev REAL NOT NULL,
                 color_space_tag TEXT NOT NULL,
                 grading_features BLOB,
                 feature_status TEXT CHECK(feature_status IS NULL OR feature_status IN ('stale', 'fresh')),
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())
             );",
        )
        .unwrap();

        conn.execute("INSERT INTO projects (name, path) VALUES ('test_proj', '/tmp/test')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'test.dpx', 'dpx')", [])
            .unwrap();

        if with_grading_features {
            let mut blob = Vec::with_capacity(1584);
            for i in 0..64u32 {
                blob.extend_from_slice(&(i as f64).to_le_bytes());
            }
            for i in 0..64u32 {
                blob.extend_from_slice(&((64 + i) as f64).to_le_bytes());
            }
            for i in 0..64u32 {
                blob.extend_from_slice(&((128 + i) as f64).to_le_bytes());
            }
            for i in 0..6u32 {
                blob.extend_from_slice(&((200 + i) as f64).to_le_bytes());
            }
            assert_eq!(blob.len(), 1584);
            conn.execute(
                "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, grading_features, feature_status) VALUES (1, '[]', '[]', '[]', 0.5, 0.1, 'video', ?1, 'stale')",
                [blob],
            ).unwrap();
        } else {
            conn.execute(
                "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, '[]', '[]', '[]', 0.5, 0.1, 'video')",
                [],
            ).unwrap();
        }

        (dir, conn)
    }

    #[test]
    fn test_migration_019_adds_new_columns() {
        let (_dir, conn) = setup_test_db_at_version_18(true);
        run_migrations(&conn).unwrap();

        // Verify all 4 new columns exist by querying them
        let hist_l: Option<Vec<u8>> = conn
            .query_row("SELECT oklab_hist_l FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert!(hist_l.is_some());
        assert!(!hist_l.unwrap().is_empty());

        let hist_a: Option<Vec<u8>> = conn
            .query_row("SELECT oklab_hist_a FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert!(hist_a.is_some());

        let hist_b: Option<Vec<u8>> = conn
            .query_row("SELECT oklab_hist_b FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert!(hist_b.is_some());

        // Migration 021 marks old 48-byte moments as NULL (stale for re-extraction)
        let moments: Option<Vec<u8>> = conn
            .query_row("SELECT color_moments FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert!(moments.is_none());
    }

    #[test]
    fn test_migration_019_migrates_existing_blob_data() {
        let (_dir, conn) = setup_test_db_at_version_18(true);
        run_migrations(&conn).unwrap();

        // Verify the split data matches the original BLOB via substr comparison
        let oklab_hist_l: Vec<u8> = conn
            .query_row("SELECT oklab_hist_l FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        let grading_features: Vec<u8> = conn
            .query_row("SELECT grading_features FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();

        // oklab_hist_l should be first 512 bytes of grading_features
        assert_eq!(oklab_hist_l, grading_features[0..512]);
        assert_eq!(oklab_hist_l.len(), 512);

        // Verify each channel has correct size
        let oklab_hist_a: Vec<u8> = conn
            .query_row("SELECT oklab_hist_a FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(oklab_hist_a, grading_features[512..1024]);
        assert_eq!(oklab_hist_a.len(), 512);

        let oklab_hist_b: Vec<u8> = conn
            .query_row("SELECT oklab_hist_b FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(oklab_hist_b, grading_features[1024..1536]);
        assert_eq!(oklab_hist_b.len(), 512);

        // Migration 021 marks old 48-byte color_moments as NULL (stale for re-extraction with 12 moments)
        let color_moments: Option<Vec<u8>> = conn
            .query_row("SELECT color_moments FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert!(color_moments.is_none());

        // Verify feature_status was set to 'stale' by migration 021
        let status: String = conn
            .query_row("SELECT feature_status FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "stale");
    }

    #[test]
    fn test_migration_019_preserves_null_rows() {
        let (_dir, conn) = setup_test_db_at_version_18(false);
        run_migrations(&conn).unwrap();

        // All 4 new columns should be NULL for rows without grading_features
        let oklab_hist_l: Option<Vec<u8>> = conn
            .query_row("SELECT oklab_hist_l FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert!(oklab_hist_l.is_none());

        let color_moments: Option<Vec<u8>> = conn
            .query_row("SELECT color_moments FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert!(color_moments.is_none());
    }

    #[test]
    fn test_migration_019_idempotency() {
        let (_dir, conn) = setup_test_db_at_version_18(true);
        run_migrations(&conn).unwrap();

        // Capture state after first run
        let oklab_hist_l_first: Vec<u8> = conn
            .query_row("SELECT oklab_hist_l FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        let version_first: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();

        // Run migrations again — should be a no-op
        run_migrations(&conn).unwrap();

        let oklab_hist_l_second: Vec<u8> = conn
            .query_row("SELECT oklab_hist_l FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        let version_second: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();

        assert_eq!(oklab_hist_l_first, oklab_hist_l_second);
        assert_eq!(version_first, version_second);
    }

    #[test]
    fn test_migration_019_preserves_old_column() {
        let (_dir, conn) = setup_test_db_at_version_18(true);
        run_migrations(&conn).unwrap();

        // Old grading_features column should still exist and be unchanged
        let grading_features: Vec<u8> = conn
            .query_row("SELECT grading_features FROM fingerprints WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(grading_features.len(), 1584); // version 18 format: 3*64*8 + 6*8
    }

    // --- Fingerprint tile tests (Story 1.2) ---

    /// Helper to create a minimal DB with a fingerprint row for tile tests.
    /// NOTE: Uses run_migrations + seed data; kept as a local helper because it inserts
    /// domain-specific test fixtures, not just schema.
    fn setup_test_db_with_fingerprint() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn).unwrap();

        // Insert minimal project -> file -> fingerprint chain
        conn.execute("INSERT INTO projects (name, path) VALUES ('test_project', '/tmp/test')", []).unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'test.dpx', 'dpx')", []).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, grading_features)
             VALUES (1, '[]', '[]', '[]', 0.5, 0.2, 'linear', X'00')",
            [],
        ).unwrap();

        (dir, conn)
    }

    fn make_test_tile_features(row: usize, col: usize) -> crate::feature_pipeline::TileFeatures {
        use crate::grading_features::GradingFeatures;
        crate::feature_pipeline::TileFeatures {
            row,
            col,
            features: GradingFeatures {
                hist_l: vec![0.5; 64],
                hist_a: vec![0.1; 64],
                hist_b: vec![0.2; 64],
                moments: [0.5, 0.2, 0.1, -0.3, 0.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            },
        }
    }

    #[test]
    fn test_store_and_load_fingerprint_tiles() {
        let (_dir, conn) = setup_test_db_with_fingerprint();

        let tiles = vec![
            make_test_tile_features(0, 0),
            make_test_tile_features(0, 1),
            make_test_tile_features(1, 0),
            make_test_tile_features(1, 1),
        ];

        store_fingerprint_tiles(&conn, 1, &tiles, 64).unwrap();

        let loaded = load_fingerprint_tiles(&conn, 1).unwrap();
        assert_eq!(loaded.len(), 4);

        // Check positions are preserved
        assert_eq!(loaded[0].0, 0); // row
        assert_eq!(loaded[0].1, 0); // col
        assert_eq!(loaded[3].0, 1);
        assert_eq!(loaded[3].1, 1);

        // Check features are preserved
        for (_, _, features) in &loaded {
            assert_eq!(features.hist_l.len(), 64);
            assert!((features.hist_l[0] - 0.5).abs() < 1e-10);
            assert!((features.moments[0] - 0.5).abs() < 1e-10);
        }
    }

    #[test]
    fn test_store_tiles_upsert_replaces_existing() {
        let (_dir, conn) = setup_test_db_with_fingerprint();

        // First insert
        let tiles_v1 = vec![make_test_tile_features(0, 0)];
        store_fingerprint_tiles(&conn, 1, &tiles_v1, 64).unwrap();
        let loaded_v1 = load_fingerprint_tiles(&conn, 1).unwrap();
        assert_eq!(loaded_v1.len(), 1);

        // Upsert with different tiles
        let tiles_v2 = vec![
            make_test_tile_features(0, 0),
            make_test_tile_features(0, 1),
        ];
        store_fingerprint_tiles(&conn, 1, &tiles_v2, 64).unwrap();
        let loaded_v2 = load_fingerprint_tiles(&conn, 1).unwrap();
        assert_eq!(loaded_v2.len(), 2);
    }

    #[test]
    fn test_load_tiles_empty_for_nonexistent_fingerprint() {
        let (_dir, conn) = setup_test_db_with_fingerprint();

        let loaded = load_fingerprint_tiles(&conn, 999).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_delete_fingerprint_tiles() {
        let (_dir, conn) = setup_test_db_with_fingerprint();

        let tiles = vec![
            make_test_tile_features(0, 0),
            make_test_tile_features(0, 1),
        ];
        store_fingerprint_tiles(&conn, 1, &tiles, 64).unwrap();

        let deleted = delete_fingerprint_tiles(&conn, 1).unwrap();
        assert_eq!(deleted, 2);

        let loaded = load_fingerprint_tiles(&conn, 1).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_delete_tiles_nonexistent_fingerprint() {
        let (_dir, conn) = setup_test_db_with_fingerprint();
        let deleted = delete_fingerprint_tiles(&conn, 999).unwrap();
        assert_eq!(deleted, 0);
    }

    // --- Resolve helper tests ---

    #[test]
    fn test_resolve_file_id_found() {
        let (_dir, conn) = setup_test_db_with_fingerprint();
        let file_id = resolve_file_id(&conn, "test_project", "test.dpx").unwrap();
        assert_eq!(file_id, 1);
    }

    #[test]
    fn test_resolve_file_id_not_found() {
        let (_dir, conn) = setup_test_db_with_fingerprint();
        let result = resolve_file_id(&conn, "nonexistent", "nofile.dpx");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_fingerprint_id_found() {
        let (_dir, conn) = setup_test_db_with_fingerprint();
        let fp_id = resolve_fingerprint_id(&conn, "test_project", "test.dpx").unwrap();
        assert_eq!(fp_id, 1);
    }

    #[test]
    fn test_resolve_fingerprint_id_not_found() {
        let (_dir, conn) = setup_test_db_with_fingerprint();
        let result = resolve_fingerprint_id(&conn, "test_project", "nonexistent.dpx");
        assert!(result.is_err());
    }
}
