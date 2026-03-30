-- 024: Session branching and compaction support
-- Note: ALTER TABLE ADD COLUMN and the branch-seq index are handled in Rust
-- (ensure_schema_extensions in db.rs) because SQLite ALTER TABLE ADD COLUMN
-- has no IF NOT EXISTS and the index depends on the new columns.

CREATE TABLE IF NOT EXISTS session_branches (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    name TEXT NOT NULL DEFAULT '',
    parent_branch_id TEXT,
    branch_point_seq INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_session_branches_session ON session_branches(session_id);
