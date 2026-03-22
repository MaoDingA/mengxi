CREATE TABLE IF NOT EXISTS analytics_sessions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      TEXT NOT NULL,
    command         TEXT NOT NULL,
    args_json       TEXT NOT NULL DEFAULT '{}',
    started_at      INTEGER NOT NULL,
    ended_at        INTEGER NOT NULL DEFAULT 0,
    duration_ms     INTEGER NOT NULL DEFAULT 0,
    exit_code       INTEGER NOT NULL DEFAULT 0,
    search_to_export_ms INTEGER,
    created_at      INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE UNIQUE INDEX idx_sessions_session_id ON analytics_sessions(session_id);
CREATE INDEX idx_sessions_started ON analytics_sessions(started_at);
CREATE INDEX idx_sessions_command ON analytics_sessions(command);
