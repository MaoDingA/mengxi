-- Migration 010: Add embedding storage to fingerprints
ALTER TABLE fingerprints ADD COLUMN embedding BLOB;
ALTER TABLE fingerprints ADD COLUMN embedding_model TEXT;
