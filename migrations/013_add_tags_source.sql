-- Migration 013: Add source column to tags table
-- Distinguishes AI-generated tags from manually-added tags.
-- Existing tags default to 'ai' (all tags prior to this migration were AI-generated).
ALTER TABLE tags ADD COLUMN source TEXT NOT NULL DEFAULT 'ai';
