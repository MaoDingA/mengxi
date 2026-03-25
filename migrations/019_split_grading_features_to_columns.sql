-- 019_split_grading_features_to_columns.sql
-- Replaces single grading_features BLOB (1584 bytes) with 4 separate BLOB columns.
-- Original BLOB layout (little-endian f64, no header):
--   hist_l(512 bytes) + hist_a(512 bytes) + hist_b(512 bytes) + moments(48 bytes)
-- SQLite substr() on BLOBs is 1-indexed.
-- Existing grading_features column is preserved for backward compatibility.

ALTER TABLE fingerprints ADD COLUMN oklab_hist_l BLOB;
ALTER TABLE fingerprints ADD COLUMN oklab_hist_a BLOB;
ALTER TABLE fingerprints ADD COLUMN oklab_hist_b BLOB;
ALTER TABLE fingerprints ADD COLUMN color_moments BLOB;

UPDATE fingerprints
SET oklab_hist_l  = substr(grading_features, 1, 512),
    oklab_hist_a  = substr(grading_features, 513, 512),
    oklab_hist_b  = substr(grading_features, 1025, 512),
    color_moments = substr(grading_features, 1537, 48)
WHERE grading_features IS NOT NULL;
