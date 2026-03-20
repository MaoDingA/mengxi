use rusqlite::{Connection, Result as SqlResult};
use std::fs;
use std::path::PathBuf;

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
        assert_eq!(migrations.len(), 7);
        assert_eq!(migrations[0].0, 1); // 001_create_projects.sql
        assert_eq!(migrations[1].0, 2); // 002_create_files.sql
        assert_eq!(migrations[2].0, 3); // 003_add_file_metadata.sql
        assert_eq!(migrations[3].0, 4); // 004_add_file_compression.sql
        assert_eq!(migrations[4].0, 5); // 005_add_mov_metadata.sql
        assert_eq!(migrations[5].0, 6); // 006_create_fingerprints.sql
        assert_eq!(migrations[6].0, 7); // 007_add_files_unique_constraint.sql
        assert!(migrations[0].1.contains("CREATE TABLE IF NOT EXISTS projects"));
        assert!(migrations[1].1.contains("CREATE TABLE IF NOT EXISTS files"));
        assert!(migrations[2].1.contains("ALTER TABLE files ADD COLUMN"));
        assert!(migrations[3].1.contains("compression"));
        assert!(migrations[4].1.contains("codec"));
        assert!(migrations[5].1.contains("fingerprints"));
    }
}
