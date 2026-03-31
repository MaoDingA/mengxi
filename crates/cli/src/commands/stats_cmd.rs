use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use mengxi_core::analytics;
use mengxi_core::db;

use super::helpers::{format_duration_ms, seconds_to_datetime};


pub fn execute(user: Option<String>, period: Option<String>, format: Option<String>) {
    let is_json = format.as_deref() == Some("json");

    let started_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let conn = match db::open_db() {
        Ok(c) => c,
        Err(e) => {
            if is_json {
                let output = serde_json::json!({
                    "status": "error",
                    "error": { "code": "STATS_DB_ERROR", "message": format!("Failed to open database: {}", e) }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: STATS_DB_ERROR — Failed to open database: {}", e);
            }
            process::exit(1);
        }
    };

    // Parse period into since_timestamp
    let since_timestamp: Option<i64> = match period.as_deref() {
        Some("1day") => Some(started_at_unix - 86_400_000),
        Some("1week") => Some(started_at_unix - 604_800_000),
        Some("2weeks") => Some(started_at_unix - 1_209_600_000),
        Some("1month") => Some(started_at_unix - 2_592_000_000),
        Some(invalid) => {
            if is_json {
                let output = serde_json::json!({
                    "status": "error",
                    "error": { "code": "STATS_INVALID_PERIOD", "message": format!("Invalid period: '{}'. Use: 1day, 1week, 2weeks, 1month", invalid) }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: STATS_INVALID_PERIOD — Invalid period: '{}'. Use: 1day, 1week, 2weeks, 1month", invalid);
            }
            process::exit(1);
        }
        None => None,
    };

    let period_label = match period.as_deref() {
        Some(p) => p.to_string(),
        None => "all time".to_string(),
    };

    // Resolve user filter: CLI --user flag takes priority, fallback to config
    let effective_user = user.as_deref()
        .filter(|u| !u.is_empty())
        .map(|u| u.to_string())
        .or_else(|| {
            crate::config::load_or_create_config().ok().map(|c| c.general.user)
        });

    // Query session stats — user-scoped or global
    let (total_sessions, avg_duration_ms, total_searches, breakdown, recent) =
        if let Some(ref u) = effective_user {
            let user_stats = analytics::get_user_stats(&conn, u, since_timestamp).unwrap_or_else(|_| analytics::UserStats {
                user: u.clone(), session_count: 0, avg_duration_ms: 0, search_count: 0, last_session_at: None,
            });
            let breakdown = analytics::get_command_breakdown_for_user(&conn, u, since_timestamp).unwrap_or_default();
            let recent = analytics::get_sessions_for_user(&conn, u, since_timestamp, 10).unwrap_or_default();
            (user_stats.session_count, user_stats.avg_duration_ms, user_stats.search_count, breakdown, recent)
        } else {
            let count = analytics::get_session_count(&conn, since_timestamp).unwrap_or(0);
            let avg = analytics::get_average_duration_ms(&conn, since_timestamp).unwrap_or(0);
            let bd = analytics::get_command_breakdown(&conn, since_timestamp).unwrap_or_default();
            let rec = analytics::get_sessions(&conn, since_timestamp, 10).unwrap_or_default();
            // Extract total searches from command breakdown
            let searches = bd.iter().find(|(cmd, _)| cmd == "search").map(|(_, c)| *c).unwrap_or(0);
            (count, avg, searches, bd, rec)
        };

    // New metrics: hit rate, calibration, vocabulary (best-effort)
    // search_feedback and calibration_activities store timestamps in seconds,
    // but since_timestamp is in milliseconds — convert before passing.
    let since_seconds = since_timestamp.map(|ts| ts / 1000);
    let hit_rate = analytics::get_search_hit_rate(&conn, since_seconds).unwrap_or(analytics::HitRateMetrics {
        accepted: 0, rejected: 0, total: 0, rate: 0.0,
    });
    let calibration = analytics::get_calibration_metrics(&conn, since_seconds).unwrap_or_else(|_| analytics::CalibrationMetrics {
        total_corrections: 0, project_breakdown: Vec::new(), latest_correction_at: None,
    });
    let trend = analytics::get_calibration_trend(&conn).unwrap_or_default();
    let vocab = analytics::get_vocabulary_metrics(&conn).unwrap_or_else(|_| analytics::VocabularyMetrics {
        total_unique_tags: 0, new_tags_last_week: 0, top_tags: Vec::new(),
    });

    // Per-user breakdown (only when --user is NOT specified)
    let per_user = if effective_user.is_none() {
        analytics::get_per_user_breakdown(&conn, since_timestamp).unwrap_or_default()
    } else {
        Vec::new()
    };

    if is_json {
        let mut cmd_map = serde_json::Map::new();
        for (cmd, count) in &breakdown {
            cmd_map.insert(cmd.clone(), serde_json::json!(*count));
        }
        let recent_json: Vec<serde_json::Value> = recent.iter().map(|s| {
            let mut obj = serde_json::Map::new();
            obj.insert("session_id".to_string(), serde_json::json!(&s.session_id));
            obj.insert("command".to_string(), serde_json::json!(&s.command));
            obj.insert("started_at".to_string(), serde_json::json!(s.started_at));
            obj.insert("duration_ms".to_string(), serde_json::json!(s.duration_ms));
            obj.insert("exit_code".to_string(), serde_json::json!(s.exit_code));
            if let Some(ste) = s.search_to_export_ms {
                obj.insert("search_to_export_ms".to_string(), serde_json::json!(ste));
            }
            serde_json::Value::Object(obj)
        }).collect();
        // Build calibration project_breakdown as ordered map
        let mut cal_breakdown = serde_json::Map::new();
        for (k, v) in &calibration.project_breakdown {
            cal_breakdown.insert(k.clone(), serde_json::json!(*v));
        }
        // Build trend as JSON array
        let trend_json: Vec<serde_json::Value> = trend.iter().map(|tp| {
            serde_json::json!({
                "week_start": tp.week_start,
                "rate": tp.rate,
            })
        }).collect();

        let mut output_map = serde_json::Map::new();
        output_map.insert("period".to_string(), serde_json::json!(period_label));
        output_map.insert("total_sessions".to_string(), serde_json::json!(total_sessions));
        output_map.insert("average_duration_ms".to_string(), serde_json::json!(avg_duration_ms));
        output_map.insert("total_searches".to_string(), serde_json::json!(total_searches));
        output_map.insert("command_breakdown".to_string(), serde_json::json!(cmd_map));
        output_map.insert("recent_sessions".to_string(), serde_json::json!(recent_json));
        output_map.insert("search_hit_rate".to_string(), serde_json::json!({
            "accepted": hit_rate.accepted,
            "rejected": hit_rate.rejected,
            "total": hit_rate.total,
            "rate": hit_rate.rate,
        }));
        output_map.insert("calibration".to_string(), serde_json::json!({
            "total_corrections": calibration.total_corrections,
            "project_breakdown": cal_breakdown,
            "latest_correction_at": calibration.latest_correction_at,
        }));
        output_map.insert("trend".to_string(), serde_json::json!(trend_json));
        output_map.insert("vocabulary".to_string(), serde_json::json!({
            "total_unique_tags": vocab.total_unique_tags,
            "new_tags_last_week": vocab.new_tags_last_week,
            "top_tags": vocab.top_tags.iter().map(|(tag, count)| serde_json::json!({"tag": tag, "count": count})).collect::<Vec<_>>(),
        }));

        // User-scoped output
        if let Some(ref u) = effective_user {
            output_map.insert("user".to_string(), serde_json::json!(u));
        }
        // Per-user breakdown
        if !per_user.is_empty() {
            let users_json: Vec<serde_json::Value> = per_user.iter().map(|us| {
                serde_json::json!({
                    "user": us.user,
                    "session_count": us.session_count,
                    "avg_duration_ms": us.avg_duration_ms,
                    "search_count": us.search_count,
                })
            }).collect();
            output_map.insert("users".to_string(), serde_json::json!(users_json));
        }

        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(output_map)).unwrap());
    } else {
        println!("Usage Statistics ({}):", period_label);
        println!("  Total sessions:    {}", total_sessions);
        println!("  Average duration:  {}", format_duration_ms(avg_duration_ms));
        println!("  Total searches:     {}", total_searches);
        if !breakdown.is_empty() {
            println!("  Command breakdown:");
            for (cmd, count) in &breakdown {
                println!("    {:12} {}", cmd, count);
            }
        }
        if !recent.is_empty() {
            println!("\nRecent sessions:");
            println!("  {:2}  {:<20} {:<10} {:<10} Status", "#", "Time", "Command", "Duration");
            for (i, s) in recent.iter().enumerate() {
                let (y, m, d, h, min, sec) = seconds_to_datetime((s.started_at / 1000) as u64);
                let time_str = format!("{}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, min, sec);
                let status = if s.exit_code == 0 { "OK" } else { "ERROR" };
                println!("  {:2}  {:<20} {:<10} {:<10} {}", i + 1, time_str, s.command, format_duration_ms(s.duration_ms), status);
            }
        }

        // Search quality metrics
        if hit_rate.total > 0 {
            println!("\nSearch Quality:");
            println!("  Acceptance rate:  {:.1}% ({} accepted, {} rejected)",
                hit_rate.rate * 100.0, hit_rate.accepted, hit_rate.rejected);
        }

        // Calibration metrics
        if calibration.total_corrections > 0 {
            println!("\nCalibration:");
            println!("  Total corrections: {}", calibration.total_corrections);
            if let Some(latest) = calibration.latest_correction_at {
                let (y, m, d, h, min, sec) = seconds_to_datetime(latest as u64);
                println!("  Latest at:         {}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, min, sec);
            }
            if !calibration.project_breakdown.is_empty() {
                println!("  Top projects:");
                for (proj, count) in &calibration.project_breakdown {
                    println!("    {:<20} {}", proj, count);
                }
            }
        }

        // Trend metrics
        if trend.len() >= 2 {
            let direction = if trend.last().unwrap().rate > trend.first().unwrap().rate { "improving" } else { "declining" };
            println!("\nTrend ({} weeks, {}):", trend.len(), direction);
            for tp in &trend {
                let (y, m, d, _, _, _) = seconds_to_datetime(tp.week_start as u64);
                println!("  {}-{:02}-{:02}  {:.1}%", y, m, d, tp.rate * 100.0);
            }
        }

        // Vocabulary metrics
        if vocab.total_unique_tags > 0 {
            println!("\nVocabulary:");
            println!("  Unique tags:       {}", vocab.total_unique_tags);
            if vocab.new_tags_last_week > 0 {
                println!("  New this week:     {}", vocab.new_tags_last_week);
            }
            if !vocab.top_tags.is_empty() {
                println!("  Top tags:");
                for (tag, count) in &vocab.top_tags {
                    println!("    {:<20} {}", tag, count);
                }
            }
        }

        // Per-user breakdown (only when multiple users exist and no --user filter)
        if per_user.len() > 1 && effective_user.is_none() {
            println!("\nUser Breakdown:");
            println!("  {:<20} | {:>8} | {:>13} | {:>8}", "User", "Sessions", "Avg Duration", "Searches");
            println!("  ---------------------+----------+---------------+----------");
            for us in &per_user {
                println!("  {:<20} | {:>8} | {:>13} | {:>8}",
                    us.user, us.session_count, format_duration_ms(us.avg_duration_ms), us.search_count);
            }
        }
    }
}
