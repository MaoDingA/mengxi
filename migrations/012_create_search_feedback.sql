-- Migration 012: Create search_feedback table
CREATE TABLE IF NOT EXISTS search_feedback (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    project_name    TEXT NOT NULL,
    file_path       TEXT NOT NULL,
    file_format     TEXT NOT NULL,
    action          TEXT NOT NULL CHECK(action IN ('accepted', 'rejected')),
    search_type     TEXT,
    created_at      INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX idx_feedback_project ON search_feedback(project_name);
CREATE INDEX idx_feedback_created ON search_feedback(created_at);
CREATE UNIQUE INDEX idx_feedback_unique_entry ON search_feedback(project_name, file_path);
