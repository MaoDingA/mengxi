// analytics.rs — Session tracking and usage analytics

use rusqlite::{Connection, OptionalExtension};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from analytics operations.
#[derive(Debug, thiserror::Error)]
pub enum AnalyticsError {
    /// A database error occurred.
    #[error("ANALYTICS_DB_ERROR -- {0}")]
    DatabaseError(String),
}

// ---------------------------------------------------------------------------
// Session record
// ---------------------------------------------------------------------------

/// A recorded CLI session.
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub command: String,
    pub args_json: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration_ms: i64,
    pub exit_code: i32,
    pub search_to_export_ms: Option<i64>,
    pub user: String,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Record a session.
pub fn record_session(
    conn: &Connection,
    record: &SessionRecord,
) -> Result<(), AnalyticsError> {
    conn.execute(
        "INSERT OR REPLACE INTO analytics_sessions (session_id, command, args_json, started_at, ended_at, duration_ms, exit_code, search_to_export_ms, user)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            record.session_id,
            record.command,
            record.args_json,
            record.started_at,
            record.ended_at,
            record.duration_ms,
            record.exit_code,
            record.search_to_export_ms,
            record.user,
        ],
    )
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(())
}/// Get the started_at timestamp of the most recent search session.
pub fn get_last_search_started_at(conn: &Connection) -> Result<Option<i64>, AnalyticsError> {
    let mut stmt = conn
        .prepare(
            "SELECT started_at FROM analytics_sessions WHERE command = 'search' ORDER BY started_at DESC LIMIT 1",
        )
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let result = stmt
        .query_row([], |row| row.get(0))
        .optional()
        .map_err(|e: rusqlite::Error| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(result.flatten())
}

/// Get session count within a time range.
pub fn get_session_count(conn: &Connection, since_timestamp: Option<i64>) -> Result<usize, AnalyticsError> {
    let count: i64 = match since_timestamp {
        Some(since) => conn
            .query_row(
                "SELECT COUNT(*) FROM analytics_sessions WHERE started_at >= ?1",
                rusqlite::params![since],
                |row| row.get(0),
            ),
        None => conn
            .query_row(
                "SELECT COUNT(*) FROM analytics_sessions",
                rusqlite::params![],
                |row| row.get(0),
            ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(count as usize)
}

/// Get average session duration in milliseconds within a time range.
pub fn get_average_duration_ms(conn: &Connection, since_timestamp: Option<i64>) -> Result<i64, AnalyticsError> {
    let avg: f64 = match since_timestamp {
        Some(since) => conn
            .query_row(
                "SELECT COALESCE(AVG(duration_ms), 0) FROM analytics_sessions WHERE started_at >= ?1 AND exit_code = 0",
                rusqlite::params![since],
                |row| row.get(0),
            ),
        None => conn
            .query_row(
                "SELECT COALESCE(AVG(duration_ms), 0) FROM analytics_sessions WHERE exit_code = 0",
                rusqlite::params![],
                |row| row.get(0),
            ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(avg as i64)
}

/// Get command breakdown (command -> count) within a time range.
pub fn get_command_breakdown(
    conn: &Connection,
    since_timestamp: Option<i64>,
) -> Result<Vec<(String, usize)>, AnalyticsError> {
    let mut stmt = match since_timestamp {
        Some(_) => conn.prepare(
            "SELECT command, COUNT(*) as cnt FROM analytics_sessions WHERE started_at >= ?1 GROUP BY command ORDER BY cnt DESC",
        ),
        None => conn.prepare(
            "SELECT command, COUNT(*) as cnt FROM analytics_sessions GROUP BY command ORDER BY cnt DESC",
        ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let rows: Vec<(String, usize)> = match since_timestamp {
        Some(since) => stmt
            .query_map(rusqlite::params![since], |row| {
                let command: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((command, count as usize))
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
        None => stmt
            .query_map(rusqlite::params![], |row| {
                let command: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((command, count as usize))
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
    };

    Ok(rows)
}

/// Get recent sessions within a time range, ordered by most recent first.
pub fn get_sessions(
    conn: &Connection,
    since_timestamp: Option<i64>,
    limit: usize,
) -> Result<Vec<SessionRecord>, AnalyticsError> {
    let mut stmt = match since_timestamp {
        Some(_) => conn.prepare(
            "SELECT session_id, command, args_json, started_at, ended_at, duration_ms, exit_code, search_to_export_ms, user
             FROM analytics_sessions WHERE started_at >= ?1
             ORDER BY started_at DESC LIMIT ?2",
        ),
        None => conn.prepare(
            "SELECT session_id, command, args_json, started_at, ended_at, duration_ms, exit_code, search_to_export_ms, user
             FROM analytics_sessions ORDER BY started_at DESC LIMIT ?1",
        ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let rows: Vec<SessionRecord> = match since_timestamp {
        Some(since) => stmt
            .query_map(rusqlite::params![since, limit as i64], |row| {
                Ok(SessionRecord {
                    session_id: row.get(0)?,
                    command: row.get(1)?,
                    args_json: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    duration_ms: row.get(5)?,
                    exit_code: row.get(6)?,
                    search_to_export_ms: row.get(7)?,
                    user: row.get(8)?,
                })
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
        None => stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok(SessionRecord {
                    session_id: row.get(0)?,
                    command: row.get(1)?,
                    args_json: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    duration_ms: row.get(5)?,
                    exit_code: row.get(6)?,
                    search_to_export_ms: row.get(7)?,
                    user: row.get(8)?,
                })
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
    };

    Ok(rows)
}

// ---------------------------------------------------------------------------
// Search hit rate metrics (FR36)
// ---------------------------------------------------------------------------

/// Search hit rate metrics.
#[derive(Debug, Clone)]
pub struct HitRateMetrics {
    pub accepted: usize,
    pub rejected: usize,
    pub total: usize,
    pub rate: f64,
}

/// Get overall search hit rate (acceptance rate).
pub fn get_search_hit_rate(
    conn: &Connection,
    since_timestamp: Option<i64>,
) -> Result<HitRateMetrics, AnalyticsError> {
    let (accepted, rejected, total): (i64, i64, i64) = match since_timestamp {
        Some(since) => conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN action = 'accepted' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN action = 'rejected' THEN 1 ELSE 0 END), 0),
                COUNT(*)
             FROM search_feedback
             WHERE created_at >= ?1",
            rusqlite::params![since],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        None => conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN action = 'accepted' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN action = 'rejected' THEN 1 ELSE 0 END), 0),
                COUNT(*)
             FROM search_feedback",
            rusqlite::params![],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let rate = if total > 0 {
        accepted as f64 / total as f64
    } else {
        0.0
    };

    Ok(HitRateMetrics {
        accepted: accepted as usize,
        rejected: rejected as usize,
        total: total as usize,
        rate,
    })
}

/// Get search hit rate broken down by search type.
pub fn get_search_hit_rate_by_type(
    conn: &Connection,
    since_timestamp: Option<i64>,
) -> Result<Vec<(String, HitRateMetrics)>, AnalyticsError> {
    let mut stmt = match since_timestamp {
        Some(_) => conn.prepare(
            "SELECT
                COALESCE(search_type, 'unknown') as search_type,
                COALESCE(SUM(CASE WHEN action = 'accepted' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN action = 'rejected' THEN 1 ELSE 0 END), 0),
                COUNT(*)
             FROM search_feedback
             WHERE created_at >= ?1
             GROUP BY search_type
             ORDER BY COUNT(*) DESC",
        ),
        None => conn.prepare(
            "SELECT
                COALESCE(search_type, 'unknown') as search_type,
                COALESCE(SUM(CASE WHEN action = 'accepted' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN action = 'rejected' THEN 1 ELSE 0 END), 0),
                COUNT(*)
             FROM search_feedback
             GROUP BY search_type
             ORDER BY COUNT(*) DESC",
        ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let rows: Vec<(String, HitRateMetrics)> = match since_timestamp {
        Some(since) => stmt
            .query_map(rusqlite::params![since], |row| {
                let search_type: String = row.get(0)?;
                let accepted: i64 = row.get(1)?;
                let rejected: i64 = row.get(2)?;
                let total: i64 = row.get(3)?;
                let rate = if total > 0 { accepted as f64 / total as f64 } else { 0.0 };
                Ok((search_type, HitRateMetrics {
                    accepted: accepted as usize,
                    rejected: rejected as usize,
                    total: total as usize,
                    rate,
                }))
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
        None => stmt
            .query_map(rusqlite::params![], |row| {
                let search_type: String = row.get(0)?;
                let accepted: i64 = row.get(1)?;
                let rejected: i64 = row.get(2)?;
                let total: i64 = row.get(3)?;
                let rate = if total > 0 { accepted as f64 / total as f64 } else { 0.0 };
                Ok((search_type, HitRateMetrics {
                    accepted: accepted as usize,
                    rejected: rejected as usize,
                    total: total as usize,
                    rate,
                }))
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
    };

    Ok(rows)
}

// ---------------------------------------------------------------------------
// Calibration metrics (FR37)
// ---------------------------------------------------------------------------

/// Calibration activity metrics.
#[derive(Debug, Clone)]
pub struct CalibrationMetrics {
    pub total_corrections: usize,
    pub project_breakdown: Vec<(String, usize)>,
    pub latest_correction_at: Option<i64>,
}

/// Get calibration activity metrics.
pub fn get_calibration_metrics(
    conn: &Connection,
    since_timestamp: Option<i64>,
) -> Result<CalibrationMetrics, AnalyticsError> {
    let total: i64 = match since_timestamp {
        Some(since) => conn.query_row(
            "SELECT COUNT(*) FROM calibration_activities WHERE created_at >= ?1",
            rusqlite::params![since],
            |row| row.get(0),
        ),
        None => conn.query_row(
            "SELECT COUNT(*) FROM calibration_activities",
            rusqlite::params![],
            |row| row.get(0),
        ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let breakdown = match since_timestamp {
        Some(since) => {
            let mut stmt = conn.prepare(
                "SELECT project_name, COUNT(*) as cnt FROM calibration_activities WHERE created_at >= ?1 GROUP BY project_name ORDER BY cnt DESC",
            ).map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;
            let result = stmt.query_map(rusqlite::params![since], |row| {
                let name: String = row.get(0)?;
                let cnt: i64 = row.get(1)?;
                Ok((name, cnt as usize))
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;
            result
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT project_name, COUNT(*) as cnt FROM calibration_activities GROUP BY project_name ORDER BY cnt DESC",
            ).map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;
            let result = stmt.query_map(rusqlite::params![], |row| {
                let name: String = row.get(0)?;
                let cnt: i64 = row.get(1)?;
                Ok((name, cnt as usize))
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;
            result
        }
    };

    let latest: Option<i64> = match since_timestamp {
        Some(since) => conn
            .query_row(
                "SELECT MAX(created_at) FROM calibration_activities WHERE created_at >= ?1",
                rusqlite::params![since],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e: rusqlite::Error| AnalyticsError::DatabaseError(e.to_string()))?
            .flatten(),
        None => conn
            .query_row(
                "SELECT MAX(created_at) FROM calibration_activities",
                rusqlite::params![],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e: rusqlite::Error| AnalyticsError::DatabaseError(e.to_string()))?
            .flatten(),
    };

    Ok(CalibrationMetrics {
        total_corrections: total as usize,
        project_breakdown: breakdown,
        latest_correction_at: latest,
    })
}

// ---------------------------------------------------------------------------
// Calibration trend (FR25)
// ---------------------------------------------------------------------------

/// A single trend data point: week start timestamp and acceptance rate.
#[derive(Debug, Clone)]
pub struct TrendPoint {
    pub week_start: i64,
    pub rate: f64,
}

/// Get weekly acceptance rate trend over time.
/// Groups feedback by week (7-day windows) and computes acceptance rate per week.
pub fn get_calibration_trend(conn: &Connection) -> Result<Vec<TrendPoint>, AnalyticsError> {
    let mut stmt = conn
        .prepare(
            "SELECT
                (created_at / 604800) * 604800 as week_start,
                SUM(CASE WHEN action = 'accepted' THEN 1 ELSE 0 END) as accepted,
                COUNT(*) as total
             FROM search_feedback
             GROUP BY week_start
             ORDER BY week_start",
        )
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let rows: Vec<TrendPoint> = stmt
        .query_map(rusqlite::params![], |row| {
            let week_start: i64 = row.get(0)?;
            let accepted: i64 = row.get(1)?;
            let total: i64 = row.get(2)?;
            let rate = if total > 0 { accepted as f64 / total as f64 } else { 0.0 };
            Ok(TrendPoint { week_start, rate })
        })
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(rows)
}

// ---------------------------------------------------------------------------
// Vocabulary metrics (FR26)
// ---------------------------------------------------------------------------

/// Vocabulary metrics: total unique tags, recent growth, top tags by frequency.
#[derive(Debug, Clone)]
pub struct VocabularyMetrics {
    pub total_unique_tags: usize,
    pub new_tags_last_week: usize,
    pub top_tags: Vec<(String, usize)>,
}

/// Get vocabulary metrics from tags table and calibration_activities.
pub fn get_vocabulary_metrics(
    conn: &Connection,
) -> Result<VocabularyMetrics, AnalyticsError> {
    // Total unique tags (manual + calibration-added)
    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (
                SELECT DISTINCT tag FROM tags WHERE source = 'manual'
                UNION
                SELECT DISTINCT value FROM calibration_activities, json_each(added_tags)
            )",
            rusqlite::params![],
            |row| row.get(0),
        )
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    // New tags added in last 7 days
    let one_week_ago = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64) - 604800;

    let new_tags: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (
                SELECT DISTINCT tag FROM tags WHERE source = 'manual' AND created_at >= ?1
                UNION
                SELECT DISTINCT value FROM calibration_activities, json_each(added_tags) WHERE calibration_activities.created_at >= ?1
            )",
            rusqlite::params![one_week_ago],
            |row| row.get(0),
        )
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    // Top tags by frequency (reuse from calibration::get_personalized_tags pattern)
    let mut stmt = conn
        .prepare(
            "SELECT tag, SUM(cnt) as total FROM (
                SELECT tag, COUNT(*) as cnt FROM tags WHERE source = 'manual' GROUP BY tag
                UNION ALL
                SELECT value, 1 as cnt FROM calibration_activities, json_each(added_tags)
            ) GROUP BY tag
            ORDER BY total DESC
            LIMIT 10",
        )
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let top_tags: Vec<(String, usize)> = stmt
        .query_map(rusqlite::params![], |row| {
            let tag: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((tag, count as usize))
        })
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(VocabularyMetrics {
        total_unique_tags: total as usize,
        new_tags_last_week: new_tags as usize,
        top_tags,
    })
}

// ---------------------------------------------------------------------------
// User statistics (FR35)
// ---------------------------------------------------------------------------

/// Per-user statistics for session analytics.
#[derive(Debug, Clone)]
pub struct UserStats {
    pub user: String,
    pub session_count: usize,
    pub avg_duration_ms: i64,
    pub search_count: usize,
    pub last_session_at: Option<i64>,
}

/// Get statistics for a specific user, optionally filtered by time period.
pub fn get_user_stats(
    conn: &Connection,
    user: &str,
    since_timestamp: Option<i64>,
) -> Result<UserStats, AnalyticsError> {
    let (session_count, avg_duration_ms, search_count, last_session_at): (i64, f64, i64, Option<i64>) =
        match since_timestamp {
            Some(since) => conn.query_row(
                "SELECT
                    COUNT(*),
                    COALESCE(AVG(CASE WHEN exit_code = 0 THEN duration_ms END), 0),
                    COALESCE(SUM(CASE WHEN command = 'search' THEN 1 ELSE 0 END), 0),
                    MAX(started_at)
                 FROM analytics_sessions
                 WHERE user = ?1 AND started_at >= ?2",
                rusqlite::params![user, since],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            ),
            None => conn.query_row(
                "SELECT
                    COUNT(*),
                    COALESCE(AVG(CASE WHEN exit_code = 0 THEN duration_ms END), 0),
                    COALESCE(SUM(CASE WHEN command = 'search' THEN 1 ELSE 0 END), 0),
                    MAX(started_at)
                 FROM analytics_sessions
                 WHERE user = ?1",
                rusqlite::params![user],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            ),
        }
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(UserStats {
        user: user.to_string(),
        session_count: session_count as usize,
        avg_duration_ms: avg_duration_ms.round() as i64,
        search_count: search_count as usize,
        last_session_at,
    })
}

/// Get all distinct user values from analytics_sessions.
pub fn get_all_users(conn: &Connection) -> Result<Vec<String>, AnalyticsError> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT user FROM analytics_sessions WHERE user != '' ORDER BY user")
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let users: Vec<String> = stmt
        .query_map(rusqlite::params![], |row| row.get(0))
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    Ok(users)
}

/// Get per-user breakdown, optionally filtered by time period.
pub fn get_per_user_breakdown(
    conn: &Connection,
    since_timestamp: Option<i64>,
) -> Result<Vec<UserStats>, AnalyticsError> {
    let mut stmt = match since_timestamp {
        Some(_) => conn.prepare(
            "SELECT
                user,
                COUNT(*),
                COALESCE(AVG(CASE WHEN exit_code = 0 THEN duration_ms END), 0),
                COALESCE(SUM(CASE WHEN command = 'search' THEN 1 ELSE 0 END), 0),
                MAX(started_at)
             FROM analytics_sessions
             WHERE user != '' AND started_at >= ?1
             GROUP BY user
             ORDER BY COUNT(*) DESC, user ASC",
        ),
        None => conn.prepare(
            "SELECT
                user,
                COUNT(*),
                COALESCE(AVG(CASE WHEN exit_code = 0 THEN duration_ms END), 0),
                COALESCE(SUM(CASE WHEN command = 'search' THEN 1 ELSE 0 END), 0),
                MAX(started_at)
             FROM analytics_sessions
             WHERE user != ''
             GROUP BY user
             ORDER BY COUNT(*) DESC, user ASC",
        ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let rows: Vec<UserStats> = match since_timestamp {
        Some(since) => stmt
            .query_map(rusqlite::params![since], |row| {
                Ok(UserStats {
                    user: row.get(0)?,
                    session_count: row.get::<_, i64>(1)? as usize,
                    avg_duration_ms: row.get::<_, f64>(2)?.round() as i64,
                    search_count: row.get::<_, i64>(3)? as usize,
                    last_session_at: row.get(4)?,
                })
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
        None => stmt
            .query_map(rusqlite::params![], |row| {
                Ok(UserStats {
                    user: row.get(0)?,
                    session_count: row.get::<_, i64>(1)? as usize,
                    avg_duration_ms: row.get::<_, f64>(2)?.round() as i64,
                    search_count: row.get::<_, i64>(3)? as usize,
                    last_session_at: row.get(4)?,
                })
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
    };

    Ok(rows)
}

/// Get command breakdown for a specific user, optionally filtered by time period.
pub fn get_command_breakdown_for_user(
    conn: &Connection,
    user: &str,
    since_timestamp: Option<i64>,
) -> Result<Vec<(String, usize)>, AnalyticsError> {
    let mut stmt = match since_timestamp {
        Some(_) => conn.prepare(
            "SELECT command, COUNT(*) as cnt FROM analytics_sessions WHERE user = ?1 AND started_at >= ?2 GROUP BY command ORDER BY cnt DESC",
        ),
        None => conn.prepare(
            "SELECT command, COUNT(*) as cnt FROM analytics_sessions WHERE user = ?1 GROUP BY command ORDER BY cnt DESC",
        ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let rows: Vec<(String, usize)> = match since_timestamp {
        Some(since) => stmt
            .query_map(rusqlite::params![user, since], |row| {
                let command: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((command, count as usize))
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
        None => stmt
            .query_map(rusqlite::params![user], |row| {
                let command: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((command, count as usize))
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
    };

    Ok(rows)
}

/// Get sessions for a specific user, optionally filtered by time period.
pub fn get_sessions_for_user(
    conn: &Connection,
    user: &str,
    since_timestamp: Option<i64>,
    limit: usize,
) -> Result<Vec<SessionRecord>, AnalyticsError> {
    let mut stmt = match since_timestamp {
        Some(_) => conn.prepare(
            "SELECT session_id, command, args_json, started_at, ended_at, duration_ms, exit_code, search_to_export_ms, user
             FROM analytics_sessions WHERE user = ?1 AND started_at >= ?2
             ORDER BY started_at DESC LIMIT ?3",
        ),
        None => conn.prepare(
            "SELECT session_id, command, args_json, started_at, ended_at, duration_ms, exit_code, search_to_export_ms, user
             FROM analytics_sessions WHERE user = ?1 ORDER BY started_at DESC LIMIT ?2",
        ),
    }
    .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?;

    let rows: Vec<SessionRecord> = match since_timestamp {
        Some(since) => stmt
            .query_map(rusqlite::params![user, since, limit as i64], |row| {
                Ok(SessionRecord {
                    session_id: row.get(0)?,
                    command: row.get(1)?,
                    args_json: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    duration_ms: row.get(5)?,
                    exit_code: row.get(6)?,
                    search_to_export_ms: row.get(7)?,
                    user: row.get(8)?,
                })
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
        None => stmt
            .query_map(rusqlite::params![user, limit as i64], |row| {
                Ok(SessionRecord {
                    session_id: row.get(0)?,
                    command: row.get(1)?,
                    args_json: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    duration_ms: row.get(5)?,
                    exit_code: row.get(6)?,
                    search_to_export_ms: row.get(7)?,
                    user: row.get(8)?,
                })
            })
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AnalyticsError::DatabaseError(e.to_string()))?,
    };

    Ok(rows)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(
            "CREATE TABLE analytics_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                command TEXT NOT NULL,
                args_json TEXT NOT NULL DEFAULT '{}',
                started_at INTEGER NOT NULL,
                ended_at INTEGER NOT NULL DEFAULT 0,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                exit_code INTEGER NOT NULL DEFAULT 0,
                search_to_export_ms INTEGER,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                user TEXT NOT NULL DEFAULT ''
            );
            CREATE UNIQUE INDEX idx_sessions_session_id ON analytics_sessions(session_id);
            CREATE INDEX idx_sessions_started ON analytics_sessions(started_at);
            CREATE INDEX idx_sessions_command ON analytics_sessions(command);
            CREATE INDEX idx_sessions_user ON analytics_sessions(user);
            CREATE TABLE search_feedback (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                file_format TEXT NOT NULL,
                action TEXT NOT NULL CHECK(action IN ('accepted', 'rejected')),
                search_type TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE INDEX idx_feedback_project ON search_feedback(project_name);
            CREATE INDEX idx_feedback_created ON search_feedback(created_at);
            CREATE UNIQUE INDEX idx_feedback_unique_entry ON search_feedback(project_name, file_path);
            CREATE TABLE calibration_activities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_name TEXT NOT NULL,
                fingerprint_id INTEGER NOT NULL,
                removed_tags TEXT NOT NULL DEFAULT '[]',
                added_tags TEXT NOT NULL DEFAULT '[]',
                renamed_tags TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE INDEX idx_calibration_project ON calibration_activities(project_name);
            CREATE INDEX idx_calibration_created ON calibration_activities(created_at);
            CREATE TABLE tags (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                fingerprint_id INTEGER NOT NULL,
                tag TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                source TEXT NOT NULL DEFAULT 'ai'
            );
            CREATE UNIQUE INDEX idx_tags_fingerprint_tag ON tags(fingerprint_id, tag);",
        )
        .unwrap();
        conn
    }

    fn ts(offset_secs: i64) -> i64 {
        1700000000 + offset_secs
    }

    #[test]
    fn test_record_session_basic() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "ses_001".to_string(),
            command: "import".to_string(),
            args_json: r#"{"name":"film"}"#.to_string(),
            started_at: ts(0),
            ended_at: ts(5000),
            duration_ms: 5000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "test_user".to_string(),
        })
        .unwrap();

        let (cmd, dur): (String, i64) = conn
            .query_row(
                "SELECT command, duration_ms FROM analytics_sessions WHERE session_id = 'ses_001'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(cmd, "import");
        assert_eq!(dur, 5000);
    }

    #[test]
    fn test_record_session_with_search_to_export() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "ses_002".to_string(),
            command: "export".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(10000),
            ended_at: ts(15000),
            duration_ms: 5000,
            exit_code: 0,
            search_to_export_ms: Some(8000),
            user: "test_user".to_string(),
        })
        .unwrap();

        let ste: Option<i64> = conn
            .query_row(
                "SELECT search_to_export_ms FROM analytics_sessions WHERE session_id = 'ses_002'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ste, Some(8000));
    }

    #[test]
    fn test_record_session_upsert() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "ses_003".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "ses_003".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(3000),
            duration_ms: 3000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        let dur: i64 = conn
            .query_row(
                "SELECT duration_ms FROM analytics_sessions WHERE session_id = 'ses_003'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dur, 3000); // updated
    }

    #[test]
    fn test_get_last_search_started_at() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(5000),
            ended_at: ts(8000),
            duration_ms: 3000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(10000),
            ended_at: ts(12000),
            duration_ms: 2000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        let result = get_last_search_started_at(&conn).unwrap();
        assert_eq!(result, Some(ts(10000))); // most recent
    }

    #[test]
    fn test_get_last_search_started_at_none() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        let result = get_last_search_started_at(&conn).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_last_search_started_at_before_export() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(8000),
            duration_ms: 8000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "export".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(20000),
            ended_at: ts(25000),
            duration_ms: 5000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        let result = get_last_search_started_at(&conn).unwrap();
        assert_eq!(result, Some(ts(0))); // still the search session, not export
    }

    #[test]
    fn test_get_session_count_all() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(2000),
            ended_at: ts(3000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(4000),
            ended_at: ts(5000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        assert_eq!(get_session_count(&conn, None).unwrap(), 3);
    }

    #[test]
    fn test_get_session_count_filtered() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(2000),
            ended_at: ts(3000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(6000),
            ended_at: ts(7000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        // Only sessions starting at or after ts(5000)
        assert_eq!(get_session_count(&conn, Some(ts(5000))).unwrap(), 1);
    }

    #[test]
    fn test_get_average_duration_ms_all() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(2000),
            duration_ms: 2000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(3000),
            ended_at: ts(5000),
            duration_ms: 2000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(6000),
            ended_at: ts(11000),
            duration_ms: 5000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        // Average of 2000 + 2000 + 5000 = 3000
        assert_eq!(get_average_duration_ms(&conn, None).unwrap(), 3000);
    }

    #[test]
    fn test_get_average_duration_ms_filtered() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(2000),
            duration_ms: 2000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(3000),
            ended_at: ts(5000),
            duration_ms: 2000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(6000),
            ended_at: ts(11000),
            duration_ms: 5000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        // Only sessions starting at or after ts(5000): avg of [5000] = 5000
        assert_eq!(get_average_duration_ms(&conn, Some(ts(5000))).unwrap(), 5000);
    }

    #[test]
    fn test_get_average_duration_ms_empty() {
        let conn = setup_test_db();
        // No sessions with exit_code = 0
        assert_eq!(get_average_duration_ms(&conn, None).unwrap(), 0);
    }

    #[test]
    fn test_get_command_breakdown_all() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(2000),
            ended_at: ts(3000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(4000),
            ended_at: ts(5000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s4".to_string(),
            command: "tag".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(6000),
            ended_at: ts(7000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        let breakdown = get_command_breakdown(&conn, None).unwrap();
        assert_eq!(breakdown.len(), 3); // import(2), search(1), tag(1)
        assert_eq!(breakdown[0], ("import".to_string(), 2));
        assert_eq!(breakdown[1], ("search".to_string(), 1));
        assert_eq!(breakdown[2], ("tag".to_string(), 1));
    }

    #[test]
    fn test_get_command_breakdown_filtered() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(2000),
            ended_at: ts(3000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(6000),
            ended_at: ts(7000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        let breakdown = get_command_breakdown(&conn, Some(ts(5000))).unwrap();
        assert_eq!(breakdown.len(), 1);
        assert_eq!(breakdown[0], ("import".to_string(), 1));
    }

    #[test]
    fn test_get_sessions_all() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(2000),
            ended_at: ts(3000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        let sessions = get_sessions(&conn, None, 10).unwrap();
        assert_eq!(sessions.len(), 2);
        // Most recent first
        assert_eq!(sessions[0].session_id, "s2");
        assert_eq!(sessions[1].session_id, "s1");
        assert_eq!(sessions[0].command, "search");
    }

    #[test]
    fn test_get_sessions_with_limit() {
        let conn = setup_test_db();
        for i in 0..5 {
            record_session(&conn, &SessionRecord {
            session_id: format!("s{}", i),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(i * 1000),
            ended_at: ts(i * 1000 + 500),
            duration_ms: 500,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        })
        .unwrap();
        }

        let sessions = get_sessions(&conn, None, 3).unwrap();
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].session_id, "s4"); // most recent
    }

    #[test]
    fn test_get_sessions_with_filter() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(5000),
            ended_at: ts(6000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(10000),
            ended_at: ts(11000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "default".to_string(),
        }).unwrap();

        let sessions = get_sessions(&conn, Some(ts(6000)), 10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s3");
    }

    #[test]
    fn test_error_display() {
        let err = AnalyticsError::DatabaseError("query failed".to_string());
        assert!(format!("{}", err).contains("ANALYTICS_DB_ERROR"));
    }

    // --- Search hit rate tests ---

    #[test]
    fn test_get_search_hit_rate_basic() {
        let conn = setup_test_db();
        // Insert feedback: 8 accepted, 2 rejected
        for i in 0..8 {
            conn.execute(
                "INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', ?1, 'dpx', 'accepted', ?2)",
                rusqlite::params![format!("f{}.dpx", i), ts(i * 100)],
            ).unwrap();
        }
        for i in 0..2 {
            conn.execute(
                "INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', ?1, 'dpx', 'rejected', ?2)",
                rusqlite::params![format!("r{}.dpx", i), ts(i * 100)],
            ).unwrap();
        }

        let hr = get_search_hit_rate(&conn, None).unwrap();
        assert_eq!(hr.accepted, 8);
        assert_eq!(hr.rejected, 2);
        assert_eq!(hr.total, 10);
        assert!((hr.rate - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_get_search_hit_rate_no_data() {
        let conn = setup_test_db();
        let hr = get_search_hit_rate(&conn, None).unwrap();
        assert_eq!(hr.accepted, 0);
        assert_eq!(hr.rejected, 0);
        assert_eq!(hr.total, 0);
        assert!((hr.rate - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_get_search_hit_rate_all_rejected() {
        let conn = setup_test_db();
        for i in 0..4 {
            conn.execute(
                "INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', ?1, 'dpx', 'rejected', ?2)",
                rusqlite::params![format!("r{}.dpx", i), ts(i)],
            ).unwrap();
        }
        let hr = get_search_hit_rate(&conn, None).unwrap();
        assert_eq!(hr.accepted, 0);
        assert_eq!(hr.rejected, 4);
        assert_eq!(hr.total, 4);
        assert!((hr.rate - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_get_search_hit_rate_all_accepted() {
        let conn = setup_test_db();
        for i in 0..5 {
            conn.execute(
                "INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', ?1, 'dpx', 'accepted', ?2)",
                rusqlite::params![format!("f{}.dpx", i), ts(i)],
            ).unwrap();
        }
        let hr = get_search_hit_rate(&conn, None).unwrap();
        assert_eq!(hr.accepted, 5);
        assert_eq!(hr.rejected, 0);
        assert!((hr.rate - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_get_search_hit_rate_filtered() {
        let conn = setup_test_db();
        // 3 accepted before filter, 2 accepted after filter
        for i in 0..3 {
            conn.execute(
                "INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', ?1, 'dpx', 'accepted', ?2)",
                rusqlite::params![format!("f{}.dpx", i), ts(1000 + i)],
            ).unwrap();
        }
        for i in 0..2 {
            conn.execute(
                "INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', ?1, 'dpx', 'accepted', ?2)",
                rusqlite::params![format!("g{}.dpx", i), ts(5000 + i)],
            ).unwrap();
        }

        let hr = get_search_hit_rate(&conn, Some(ts(4500))).unwrap();
        assert_eq!(hr.total, 2);
    }

    #[test]
    fn test_get_search_hit_rate_by_type() {
        let conn = setup_test_db();
        // 3 image accepted, 1 image rejected
        for i in 0..3 {
            conn.execute(
                "INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, search_type, created_at) VALUES ('p', ?1, 'dpx', 'accepted', 'image', ?2)",
                rusqlite::params![format!("f{}.dpx", i), ts(i)],
            ).unwrap();
        }
        conn.execute(
            "INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, search_type, created_at) VALUES ('p', 'rx.dpx', 'dpx', 'rejected', 'image', 10)",
            [],
        ).unwrap();
        // 2 tag accepted
        for i in 0..2 {
            conn.execute(
                "INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, search_type, created_at) VALUES ('p', ?1, 'dpx', 'accepted', 'tag', 20)",
                rusqlite::params![format!("t{}.dpx", i)],
            ).unwrap();
        }

        let by_type = get_search_hit_rate_by_type(&conn, None).unwrap();
        assert_eq!(by_type.len(), 2);
        // image: 3 accepted, 1 rejected = 75%
        let image = by_type.iter().find(|(t, _)| t == "image").unwrap();
        assert_eq!(image.1.accepted, 3);
        assert_eq!(image.1.rejected, 1);
        assert!((image.1.rate - 0.75).abs() < 0.001);
        // tag: 2 accepted, 0 rejected = 100%
        let tag = by_type.iter().find(|(t, _)| t == "tag").unwrap();
        assert_eq!(tag.1.accepted, 2);
        assert!((tag.1.rate - 1.0).abs() < 0.001);
    }

    // --- Calibration metrics tests ---

    #[test]
    fn test_get_calibration_metrics_basic() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO calibration_activities (project_name, fingerprint_id, added_tags, created_at) VALUES ('film_a', 1, '[]', 1000)", []).unwrap();
        conn.execute("INSERT INTO calibration_activities (project_name, fingerprint_id, added_tags, created_at) VALUES ('film_a', 1, '[]', 2000)", []).unwrap();
        conn.execute("INSERT INTO calibration_activities (project_name, fingerprint_id, added_tags, created_at) VALUES ('film_b', 1, '[]', 3000)", []).unwrap();

        let metrics = get_calibration_metrics(&conn, None).unwrap();
        assert_eq!(metrics.total_corrections, 3);
        assert_eq!(metrics.project_breakdown.len(), 2);
        assert_eq!(metrics.project_breakdown[0], ("film_a".to_string(), 2));
        assert_eq!(metrics.project_breakdown[1], ("film_b".to_string(), 1));
        assert_eq!(metrics.latest_correction_at, Some(3000));
    }

    #[test]
    fn test_get_calibration_metrics_empty() {
        let conn = setup_test_db();
        let metrics = get_calibration_metrics(&conn, None).unwrap();
        assert_eq!(metrics.total_corrections, 0);
        assert!(metrics.project_breakdown.is_empty());
        assert_eq!(metrics.latest_correction_at, None);
    }

    #[test]
    fn test_get_calibration_metrics_filtered() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO calibration_activities (project_name, fingerprint_id, added_tags, created_at) VALUES ('film_a', 1, '[]', ?1)", rusqlite::params![ts(1000)]).unwrap();
        conn.execute("INSERT INTO calibration_activities (project_name, fingerprint_id, added_tags, created_at) VALUES ('film_b', 1, '[]', ?1)", rusqlite::params![ts(5000)]).unwrap();

        let metrics = get_calibration_metrics(&conn, Some(ts(3000))).unwrap();
        assert_eq!(metrics.total_corrections, 1);
        assert_eq!(metrics.project_breakdown[0], ("film_b".to_string(), 1));
    }

    // --- Trend tests ---

    #[test]
    fn test_get_calibration_trend_basic() {
        let conn = setup_test_db();
        // Week 1 (ts 0-604799): 3 accepted, 1 rejected = 75%
        conn.execute("INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', 'f1.dpx', 'dpx', 'accepted', 1000)", []).unwrap();
        conn.execute("INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', 'f2.dpx', 'dpx', 'accepted', 2000)", []).unwrap();
        conn.execute("INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', 'f3.dpx', 'dpx', 'accepted', 3000)", []).unwrap();
        conn.execute("INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', 'f4.dpx', 'dpx', 'rejected', 4000)", []).unwrap();
        // Week 2 (ts 604800-1209599): 1 accepted, 0 rejected = 100%
        conn.execute("INSERT OR IGNORE INTO search_feedback (project_name, file_path, file_format, action, created_at) VALUES ('p', 'f5.dpx', 'dpx', 'accepted', 700000)", []).unwrap();

        let trend = get_calibration_trend(&conn).unwrap();
        assert_eq!(trend.len(), 2);
        assert!((trend[0].rate - 0.75).abs() < 0.001);
        assert!((trend[1].rate - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_get_calibration_trend_no_data() {
        let conn = setup_test_db();
        let trend = get_calibration_trend(&conn).unwrap();
        assert!(trend.is_empty());
    }

    // --- Vocabulary metrics tests ---

    #[test]
    fn test_get_vocabulary_metrics_basic() {
        let conn = setup_test_db();
        // Manual tags
        conn.execute("INSERT INTO tags (fingerprint_id, tag, source, created_at) VALUES (1, 'warm', 'manual', 1000)", []).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag, source, created_at) VALUES (1, 'cool', 'manual', 2000)", []).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag, source, created_at) VALUES (1, 'industrial', 'ai', 3000)", []).unwrap();
        // Calibration added tags
        conn.execute("INSERT INTO calibration_activities (project_name, fingerprint_id, added_tags, created_at) VALUES ('p', 1, '[\"warm\", \"moody\"]', 4000)", []).unwrap();

        let metrics = get_vocabulary_metrics(&conn).unwrap();
        // warm (manual + calibration), cool (manual), moody (calibration) = 3 unique
        // industrial is ai, not counted
        assert_eq!(metrics.total_unique_tags, 3);
        // top tags: warm (count 2), moody (count 1), cool (count 1)
        assert_eq!(metrics.top_tags[0], ("warm".to_string(), 2));
    }

    #[test]
    fn test_get_vocabulary_metrics_empty() {
        let conn = setup_test_db();
        let metrics = get_vocabulary_metrics(&conn).unwrap();
        assert_eq!(metrics.total_unique_tags, 0);
        assert_eq!(metrics.new_tags_last_week, 0);
        assert!(metrics.top_tags.is_empty());
    }

    // --- User stats tests ---

    #[test]
    fn test_get_user_stats_basic() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(2000),
            ended_at: ts(3000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(4000),
            ended_at: ts(5000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "bob".to_string(),
        }).unwrap();

        let stats = get_user_stats(&conn, "alice", None).unwrap();
        assert_eq!(stats.user, "alice");
        assert_eq!(stats.session_count, 2);
        assert_eq!(stats.avg_duration_ms, 1000);
        assert_eq!(stats.search_count, 1);
        assert_eq!(stats.last_session_at, Some(ts(2000)));
    }

    #[test]
    fn test_get_user_stats_filtered_by_period() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(5000),
            ended_at: ts(6000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();

        let stats = get_user_stats(&conn, "alice", Some(ts(3000))).unwrap();
        assert_eq!(stats.session_count, 1);
    }

    #[test]
    fn test_get_user_stats_no_sessions() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();

        let stats = get_user_stats(&conn, "nonexistent", None).unwrap();
        assert_eq!(stats.user, "nonexistent");
        assert_eq!(stats.session_count, 0);
        assert_eq!(stats.avg_duration_ms, 0);
        assert_eq!(stats.search_count, 0);
        assert_eq!(stats.last_session_at, None);
    }

    #[test]
    fn test_get_all_users() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(2000),
            ended_at: ts(3000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "bob".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "tag".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(4000),
            ended_at: ts(5000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();
        // Empty user should be excluded
        record_session(&conn, &SessionRecord {
            session_id: "s4".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(6000),
            ended_at: ts(7000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "".to_string(),
        }).unwrap();

        let users = get_all_users(&conn).unwrap();
        assert_eq!(users.len(), 2);
        assert_eq!(users[0], "alice");
        assert_eq!(users[1], "bob");
    }

    #[test]
    fn test_get_all_users_empty() {
        let conn = setup_test_db();
        let users = get_all_users(&conn).unwrap();
        assert!(users.is_empty());
    }

    #[test]
    fn test_get_per_user_breakdown() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(2000),
            ended_at: ts(3000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(4000),
            ended_at: ts(5000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s4".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(6000),
            ended_at: ts(7000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "bob".to_string(),
        }).unwrap();

        let breakdown = get_per_user_breakdown(&conn, None).unwrap();
        assert_eq!(breakdown.len(), 2);
        // alice has more sessions, should be first
        assert_eq!(breakdown[0].user, "alice");
        assert_eq!(breakdown[0].session_count, 3);
        assert_eq!(breakdown[0].search_count, 2);
        assert_eq!(breakdown[1].user, "bob");
        assert_eq!(breakdown[1].session_count, 1);
    }

    #[test]
    fn test_get_per_user_breakdown_filtered() {
        let conn = setup_test_db();
        record_session(&conn, &SessionRecord {
            session_id: "s1".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(0),
            ended_at: ts(1000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s2".to_string(),
            command: "search".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(5000),
            ended_at: ts(6000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "alice".to_string(),
        }).unwrap();
        record_session(&conn, &SessionRecord {
            session_id: "s3".to_string(),
            command: "import".to_string(),
            args_json: "{}".to_string(),
            started_at: ts(10000),
            ended_at: ts(11000),
            duration_ms: 1000,
            exit_code: 0,
            search_to_export_ms: None,
            user: "bob".to_string(),
        }).unwrap();

        let breakdown = get_per_user_breakdown(&conn, Some(ts(3000))).unwrap();
        assert_eq!(breakdown.len(), 2);
        // Only alice's second session and bob's session are after ts(3000)
        // Tied on count, secondary sort by user ASC
        assert_eq!(breakdown[0].user, "alice");
        assert_eq!(breakdown[0].session_count, 1);
        assert_eq!(breakdown[1].user, "bob");
        assert_eq!(breakdown[1].session_count, 1);
    }
}
