use unicode_width::UnicodeWidthStr;

/// Truncate a string to max_len display columns, appending "…" if truncated.
/// Uses unicode-width for correct CJK/emoji column counting.
pub fn truncate_str(s: &str, max_len: usize) -> String {
    let width = UnicodeWidthStr::width(s);
    if width <= max_len {
        s.to_string()
    } else {
        let ellipsis_width = UnicodeWidthStr::width("…");
        let target = max_len.saturating_sub(ellipsis_width);
        let mut result = String::new();
        let mut current_width = 0usize;
        for ch in s.chars() {
            let ch_width = UnicodeWidthStr::width(ch.to_string().as_str());
            if current_width + ch_width > target {
                break;
            }
            result.push(ch);
            current_width += ch_width;
        }
        result.push('…');
        result
    }
}

/// Format milliseconds into human-readable duration string.
pub fn format_duration_ms(ms: i64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let total_secs = ms / 1000;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{}m {}s", mins, secs)
    }
}

/// Convert seconds since Unix epoch to (year, month, day, hour, min, sec).
/// Simple implementation to avoid chrono dependency.
/// Algorithm from http://howardhinnant.github.io/date_algorithms.html
pub fn seconds_to_datetime(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let days = (secs / 86400) as i32;
    let time_of_day = (secs % 86400) as u32;
    let hour = time_of_day / 3600;
    let min = (time_of_day % 3600) / 60;
    let sec = time_of_day % 60;

    let mut z = days + 719468;
    let era = z / 146097;
    z -= era * 146097;
    let doe = z;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y as u32, m as u32, d as u32, hour, min, sec)
}
