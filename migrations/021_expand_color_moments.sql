-- Migration 021: Expand color_moments BLOB from 48 bytes (6 moments) to 96 bytes (12 moments)
-- Old moments: [L_mean, L_std, a_mean, a_std, b_mean, b_std]
-- New moments: [L_mean, L_std, L_skew, L_kurt, a_mean, a_std, a_skew, a_kurt, b_mean, b_std, b_skew, b_kurt]
-- Existing fingerprints with old 48-byte moments are marked stale for re-extraction.

UPDATE fingerprints
SET feature_status = 'stale',
    color_moments = NULL
WHERE color_moments IS NOT NULL
  AND length(color_moments) = 48;
