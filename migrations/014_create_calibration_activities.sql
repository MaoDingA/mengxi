-- Migration 014: Create calibration_activities table
-- Records tag corrections made by the colorist for the calibration learning loop.
-- removed_tags/added_tags are JSON arrays of tag strings.
-- renamed_tags is a JSON array of {"old": "...", "new": "..."} objects.
CREATE TABLE IF NOT EXISTS calibration_activities (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    project_name    TEXT NOT NULL,
    fingerprint_id  INTEGER NOT NULL REFERENCES fingerprints(id) ON DELETE CASCADE,
    removed_tags    TEXT NOT NULL DEFAULT '[]',
    added_tags      TEXT NOT NULL DEFAULT '[]',
    renamed_tags    TEXT NOT NULL DEFAULT '[]',
    created_at      INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX idx_calibration_project ON calibration_activities(project_name);
CREATE INDEX idx_calibration_created ON calibration_activities(created_at);
