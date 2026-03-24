-- 018_add_feature_status_to_fingerprints.sql
-- Adds feature_status TEXT column to track grading feature freshness.
-- Existing rows with grading_features BLOB are marked 'stale'.
-- Rows without grading_features keep feature_status IS NULL.
-- New features from the import pipeline will be marked 'fresh' (Story 2.3 pipeline change).

ALTER TABLE fingerprints ADD COLUMN feature_status TEXT CHECK(feature_status IS NULL OR feature_status IN ('stale', 'fresh'));

UPDATE fingerprints SET feature_status = 'stale' WHERE grading_features IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_fingerprints_feature_status ON fingerprints(feature_status);
