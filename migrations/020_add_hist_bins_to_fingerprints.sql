-- 020_add_hist_bins_to_fingerprints.sql
-- Adds hist_bins INTEGER column to store histogram bin count per fingerprint.
-- Existing rows default to 64 (backward compatible with migration 019's 512-byte histograms).

ALTER TABLE fingerprints ADD COLUMN hist_bins INTEGER NOT NULL DEFAULT 64;

-- Update existing rows: set hist_bins = 64 where data exists (64-bin histograms were stored)
UPDATE fingerprints SET hist_bins = 64 WHERE oklab_hist_l IS NOT NULL;
