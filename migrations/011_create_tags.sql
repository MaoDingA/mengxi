-- Migration 011: Create tags table
CREATE TABLE IF NOT EXISTS tags (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    fingerprint_id  INTEGER NOT NULL REFERENCES fingerprints(id) ON DELETE CASCADE,
    tag             TEXT NOT NULL,
    created_at      INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE UNIQUE INDEX idx_tags_fingerprint_tag ON tags(fingerprint_id, tag);
