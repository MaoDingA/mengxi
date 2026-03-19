-- Migration 003: Add file metadata columns for format-specific header data
-- Columns are nullable — non-DPX files (EXR, MOV) will have NULL until their stories are implemented

ALTER TABLE files ADD COLUMN width INTEGER;
ALTER TABLE files ADD COLUMN height INTEGER;
ALTER TABLE files ADD COLUMN bit_depth INTEGER;
ALTER TABLE files ADD COLUMN transfer TEXT;
ALTER TABLE files ADD COLUMN colorimetric TEXT;
ALTER TABLE files ADD COLUMN descriptor TEXT;
