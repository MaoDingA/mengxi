-- Migration 022: Create fingerprint_tiles table for per-tile grading features.
-- Each row stores one tile's features: histograms (L, a, b) and color moments
-- for a specific grid position (row, col) within a fingerprint.
-- This is additive — existing global fingerprint features remain unchanged.

CREATE TABLE IF NOT EXISTS fingerprint_tiles (
    id INTEGER NOT NULL PRIMARY KEY,
    fingerprint_id INTEGER NOT NULL REFERENCES fingerprints(id) ON DELETE CASCADE,
    tile_row INTEGER NOT NULL,
    tile_col INTEGER NOT NULL,
    oklab_hist_l BLOB,
    oklab_hist_a BLOB,
    oklab_hist_b BLOB,
    color_moments BLOB,
    hist_bins INTEGER NOT NULL DEFAULT 64,
    UNIQUE(fingerprint_id, tile_row, tile_col)
);

CREATE INDEX IF NOT EXISTS idx_fingerprint_tiles_fingerprint_id
    ON fingerprint_tiles(fingerprint_id, tile_row, tile_col);
