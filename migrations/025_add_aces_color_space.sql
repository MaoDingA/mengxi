-- Migration 025: Add explicit aces_color_space column separate from color_space_tag
-- color_space_tag stores encoding (linear/log/video), NOT ACES color space names
-- This column is NULL for all existing rows — non-inferential migration
ALTER TABLE fingerprints ADD COLUMN aces_color_space TEXT;

-- Index for filtered queries (conditional on non-NULL values)
CREATE INDEX IF NOT EXISTS idx_fingerprints_aces_color_space
ON fingerprints(aces_color_space) WHERE aces_color_space IS NOT NULL;
