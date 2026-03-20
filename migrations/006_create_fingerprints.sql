CREATE TABLE IF NOT EXISTS fingerprints (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id     INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    histogram_r TEXT NOT NULL,
    histogram_g TEXT NOT NULL,
    histogram_b TEXT NOT NULL,
    luminance_mean REAL NOT NULL,
    luminance_stddev REAL NOT NULL,
    color_space_tag TEXT NOT NULL,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
