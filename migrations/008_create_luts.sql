CREATE TABLE IF NOT EXISTS luts (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id    INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    fingerprint_id INTEGER REFERENCES fingerprints(id),
    title         TEXT,
    format        TEXT NOT NULL CHECK(format IN ('cube', '3dl', 'look', 'csp', 'cdl')),
    grid_size     INTEGER NOT NULL,
    output_path   TEXT NOT NULL,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_luts_project_id ON luts(project_id);
