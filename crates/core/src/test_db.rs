//! Centralized test database setup.
//! All test modules MUST use these functions -- no inline DDL allowed.

use rusqlite::Connection;
use tempfile::TempDir;

/// Run all migrations from the `migrations/` directory against an in-memory DB.
/// This is the DEFAULT for 90% of tests. Uses WAL mode + FK enforcement.
pub fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .unwrap();
    crate::db::run_migrations(&conn).expect("migrations");
    conn
}

/// File-backed DB with all migrations applied. Returns `(TempDir, Connection)`
/// so the DB file outlives the test (required for WAL mode / some rusqlite features).
pub fn setup_test_db_file() -> (TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let db_file = dir.path().join("test.db");
    let conn = Connection::open(&db_file).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .unwrap();
    crate::db::run_migrations(&conn).expect("migrations");
    (dir, conn)
}
