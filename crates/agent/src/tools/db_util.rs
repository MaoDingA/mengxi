// tools/db_util.rs — Shared DB access and result serialization for agent tools

use crate::tool::ToolError;
use mengxi_core::search::SearchOptions;
use mengxi_core::search::SearchResult;
use mengxi_core::hybrid_scoring::HybridSearchResult;
use serde_json::{json, Value};

/// Open the mengxi database connection with centralized error handling.
pub fn open_connection() -> Result<mengxi_core::db::DbConnection, ToolError> {
    mengxi_core::db::open_db()
        .map_err(|e| ToolError::ExecutionError(format!("DB_OPEN_ERROR -- {}", e)))
}

/// Build SearchOptions from tool JSON parameters.
pub fn build_search_options(params: &Value, default_limit: usize) -> SearchOptions {
    SearchOptions {
        project: params
            .get("project")
            .and_then(|v| v.as_str())
            .map(String::from),
        limit: params
            .get("limit")
            .and_then(|v| v.as_i64())
            .map(|v| v as usize)
            .unwrap_or(default_limit),
        use_pyramid: params
            .get("use_pyramid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

/// Round a float to 2 decimal places.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Serialize basic search results to JSON.
pub fn search_results_to_json(results: &[SearchResult], tool_name: &str) -> Value {
    json!({
        "tool": tool_name,
        "results": results.iter().map(|r| json!({
            "rank": r.rank,
            "project": r.project_name,
            "file": r.file_path,
            "format": r.file_format,
            "score": round2(r.score),
        })).collect::<Vec<_>>(),
        "total": results.len(),
    })
}

/// Serialize hybrid search results to JSON.
pub fn hybrid_results_to_json(results: &[HybridSearchResult], tool_name: &str) -> Value {
    json!({
        "tool": tool_name,
        "results": results.iter().map(|r| json!({
            "rank": r.rank,
            "project": r.project_name,
            "file": r.file_path,
            "format": r.file_format,
            "score": round2(r.score),
            "breakdown": {
                "grading": round2(r.score_breakdown.grading),
                "clip": r.score_breakdown.clip.map(round2),
                "tag": r.score_breakdown.tag.map(round2),
            },
            "warnings": r.match_warnings,
            "description": r.human_readable,
        })).collect::<Vec<_>>(),
        "total": results.len(),
    })
}

/// Format a text summary of search results for the LLM content field.
pub fn format_results_summary(results: &[Value]) -> String {
    if results.is_empty() {
        return "No matching results found.".to_string();
    }
    let mut out = format!("Found {} results:\n", results.len());
    for r in results {
        out.push_str(&format!(
            "  #{}: {} ({}) -- score: {}\n",
            r["rank"], r["file"], r["project"], r["score"]
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mengxi_core::hybrid_scoring::ScoreBreakdown;

    #[test]
    fn test_round2() {
        assert_eq!(round2(0.95333333), 0.95);
        assert_eq!(round2(1.0), 1.0);
        assert_eq!(round2(0.0), 0.0);
    }

    #[test]
    fn test_build_search_options_defaults() {
        let params = json!({});
        let opts = build_search_options(&params, 10);
        assert!(opts.project.is_none());
        assert_eq!(opts.limit, 10);
        assert!(!opts.use_pyramid);
    }

    #[test]
    fn test_build_search_options_from_params() {
        let params = json!({"project": "film_a", "limit": 5, "use_pyramid": true});
        let opts = build_search_options(&params, 10);
        assert_eq!(opts.project.as_deref(), Some("film_a"));
        assert_eq!(opts.limit, 5);
        assert!(opts.use_pyramid);
    }

    #[test]
    fn test_search_results_to_json_structure() {
        let results = vec![SearchResult {
            rank: 1,
            project_name: "film_a".into(),
            file_path: "scene001.dpx".into(),
            file_format: "dpx".into(),
            score: 0.95333333,
        }];
        let json = search_results_to_json(&results, "test_tool");
        assert_eq!(json["tool"], "test_tool");
        assert_eq!(json["total"], 1);
        assert_eq!(json["results"][0]["score"], 0.95);
        assert_eq!(json["results"][0]["project"], "film_a");
    }

    #[test]
    fn test_format_results_summary_empty() {
        let summary = format_results_summary(&[]);
        assert_eq!(summary, "No matching results found.");
    }

    #[test]
    fn test_format_results_summary_with_data() {
        let results = vec![json!({"rank": 1, "file": "s.dpx", "project": "film", "score": 0.95})];
        let summary = format_results_summary(&results);
        assert!(summary.contains("Found 1 results"));
        assert!(summary.contains("s.dpx"));
    }

    #[test]
    fn test_hybrid_results_to_json_structure() {
        let results = vec![HybridSearchResult {
            rank: 1,
            project_name: "film_a".into(),
            file_path: "scene.dpx".into(),
            file_format: "dpx".into(),
            score: 0.88,
            score_breakdown: ScoreBreakdown {
                grading: 0.6,
                clip: Some(0.9),
                tag: Some(0.5),
            },
            match_warnings: vec![],
            human_readable: "High similarity".into(),
        }];
        let json = hybrid_results_to_json(&results, "hybrid");
        assert_eq!(json["total"], 1);
        assert_eq!(json["results"][0]["breakdown"]["clip"], 0.9);
    }
}
