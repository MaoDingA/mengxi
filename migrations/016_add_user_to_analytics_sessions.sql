-- Migration 016: Add user identity to analytics_sessions
-- Story 5.3: Statistics Display & Reporting (FR35)
ALTER TABLE analytics_sessions ADD COLUMN user TEXT NOT NULL DEFAULT '';
CREATE INDEX idx_sessions_user ON analytics_sessions(user);
