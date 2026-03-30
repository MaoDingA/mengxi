// tools/analyze.rs — Analysis tools wired to mengxi-core APIs

use async_trait::async_trait;
use mengxi_core::comparison::{CompareError, CompareResult};
use mengxi_core::consistency::{ConsistencyError, ConsistencyReport};
use mengxi_core::search::{FingerprintInfo, SearchError};
use serde_json::{json, Value};

use crate::tool::{Tool, ToolError, ToolResult};
use crate::tools::db_util;

// --- AnalyzeProjectTool ---

pub struct AnalyzeProjectTool;

#[async_trait]
impl Tool for AnalyzeProjectTool {
    fn name(&self) -> &str {
        "analyze_project"
    }
    fn description(&self) -> &str {
        "Analyze a project's color distribution, consistency metrics, and outlier detection. \
         Returns per-project Oklab centroids, pairwise distances, and fingerprints that \
         deviate significantly from the project's average color profile."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project name to analyze" }
            },
            "required": ["project"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let project = params
            .get("project")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: project".into()))?;

        let conn = db_util::open_connection()?;
        let report = mengxi_core::consistency::generate_consistency_report(
            &conn,
            &[project.to_string()],
        )
        .map_err(|e| handle_consistency_error(e))?;

        let display = consistency_report_to_json(&report);
        let summary = format_consistency_report(&report);
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- CompareStylesTool ---

pub struct CompareStylesTool;

#[async_trait]
impl Tool for CompareStylesTool {
    fn name(&self) -> &str {
        "compare_styles"
    }
    fn description(&self) -> &str {
        "Compare two fingerprints side-by-side with detailed per-channel breakdown. \
         Shows Oklab histogram deltas (L/a/b channels), luminance differences, \
         and an overall similarity score. Also flags color space mismatches."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id_a": { "type": "integer", "description": "First fingerprint ID" },
                "id_b": { "type": "integer", "description": "Second fingerprint ID" }
            },
            "required": ["id_a", "id_b"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let id_a = params
            .get("id_a")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: id_a".into()))?;
        let id_b = params
            .get("id_b")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: id_b".into()))?;

        if id_a == id_b {
            return Err(ToolError::InvalidParams(
                "id_a and id_b must be different fingerprints".into(),
            ));
        }

        let conn = db_util::open_connection()?;
        let result =
            mengxi_core::comparison::compare_fingerprints(&conn, id_a, id_b).map_err(|e| {
                match e {
                    CompareError::NotFound(id) => ToolError::ExecutionError(format!(
                        "FINGERPRINT_NOT_FOUND -- fingerprint {} not found",
                        id
                    )),
                    CompareError::DbError(msg) => {
                        ToolError::ExecutionError(format!("DATABASE_ERROR -- {}", msg))
                    }
                }
            })?;

        let display = compare_result_to_json(&result);
        let summary = format_compare_result(&result);
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- GetFingerprintInfoTool ---

pub struct GetFingerprintInfoTool;

#[async_trait]
impl Tool for GetFingerprintInfoTool {
    fn name(&self) -> &str {
        "get_fingerprint_info"
    }
    fn description(&self) -> &str {
        "Get detailed fingerprint information including color space, luminance statistics, \
         per-channel histogram summaries (mean value, dominant range), and associated tags. \
         Use to inspect a specific file's color properties within a project."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project name" },
                "file": { "type": "string", "description": "File name within the project" }
            },
            "required": ["project", "file"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let project = params
            .get("project")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: project".into()))?;
        let file = params
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: file".into()))?;

        let conn = db_util::open_connection()?;
        let info = mengxi_core::search::fingerprint_info_with_tags(&conn, project, file)
            .map_err(|e| handle_search_error(e))?;

        let display = fingerprint_info_to_json(&info);
        let summary = format_fingerprint_info(&info);
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- Serialization helpers ---

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn consistency_report_to_json(report: &ConsistencyReport) -> Value {
    json!({
        "tool": "analyze_project",
        "overall_consistency": round2(report.overall_consistency),
        "summaries": report.project_summaries.iter().map(|s| json!({
            "project": s.name,
            "fingerprint_count": s.fingerprint_count,
            "l_centroid": round2(s.l_centroid),
            "a_centroid": round2(s.a_centroid),
            "b_centroid": round2(s.b_centroid),
        })).collect::<Vec<_>>(),
        "pair_distances": report.pair_distances.iter().map(|d| json!({
            "project_a": d.project_a,
            "project_b": d.project_b,
            "histogram_distance": round2(d.histogram_distance),
            "luminance_diff": round2(d.luminance_diff),
        })).collect::<Vec<_>>(),
        "outliers": report.outliers.iter().map(|o| json!({
            "id": o.id,
            "project": o.project,
            "file": o.file,
            "distance_from_mean": round2(o.distance_from_mean),
        })).collect::<Vec<_>>(),
    })
}

fn format_consistency_report(report: &ConsistencyReport) -> String {
    let mut out = String::new();
    for s in &report.project_summaries {
        out.push_str(&format!(
            "Project '{}' ({} fingerprints):\n",
            s.name, s.fingerprint_count
        ));
        out.push_str(&format!(
            "  Oklab centroid: L={:.2}, a={:.2}, b={:.2}\n",
            s.l_centroid, s.a_centroid, s.b_centroid
        ));
    }
    if report.outliers.is_empty() {
        out.push_str("No outliers detected.\n");
    } else {
        out.push_str(&format!("Outliers ({}):\n", report.outliers.len()));
        for o in &report.outliers {
            out.push_str(&format!(
                "  {} ({}): distance {:.2}\n",
                o.file, o.project, o.distance_from_mean
            ));
        }
    }
    out.push_str(&format!(
        "Overall consistency: {:.2} (0=identical, 1=very different)\n",
        report.overall_consistency
    ));
    out
}

fn compare_result_to_json(result: &CompareResult) -> Value {
    json!({
        "tool": "compare_styles",
        "id_a": result.id_a,
        "id_b": result.id_b,
        "file_a": result.file_a,
        "file_b": result.file_b,
        "project_a": result.project_a,
        "project_b": result.project_b,
        "color_space_a": result.color_space_a,
        "color_space_b": result.color_space_b,
        "color_space_match": result.color_space_match,
        "overall_distance": round2(result.overall_distance),
        "channels": {
            "L": histogram_delta_to_json(&result.hist_l_delta),
            "a": histogram_delta_to_json(&result.hist_a_delta),
            "b": histogram_delta_to_json(&result.hist_b_delta),
        },
        "luminance": {
            "mean_delta": round2(result.luminance_delta.mean_delta),
            "stddev_delta": round2(result.luminance_delta.stddev_delta),
        },
    })
}

fn histogram_delta_to_json(delta: &mengxi_core::comparison::HistogramDelta) -> Value {
    json!({
        "mean_abs_diff": round2(delta.mean_abs_diff),
        "max_abs_diff": round2(delta.max_abs_diff),
        "max_diff_bin": delta.max_diff_bin,
        "l1_norm": round2(delta.l1_norm),
    })
}

fn format_compare_result(result: &CompareResult) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Comparing {} ({}) vs {} ({}):\n",
        result.file_a, result.project_a, result.file_b, result.project_b
    ));
    if !result.color_space_match {
        out.push_str(&format!(
            "  WARNING: Color space mismatch ({} vs {})\n",
            result.color_space_a, result.color_space_b
        ));
    }
    out.push_str(&format!(
        "  Overall distance: {:.2} (0=identical, 1=very different)\n",
        result.overall_distance
    ));
    out.push_str(&format!(
        "  L channel: mean_diff={:.3}, max_diff={:.3} at bin {}\n",
        result.hist_l_delta.mean_abs_diff,
        result.hist_l_delta.max_abs_diff,
        result.hist_l_delta.max_diff_bin
    ));
    out.push_str(&format!(
        "  a channel: mean_diff={:.3}, max_diff={:.3} at bin {}\n",
        result.hist_a_delta.mean_abs_diff,
        result.hist_a_delta.max_abs_diff,
        result.hist_a_delta.max_diff_bin
    ));
    out.push_str(&format!(
        "  b channel: mean_diff={:.3}, max_diff={:.3} at bin {}\n",
        result.hist_b_delta.mean_abs_diff,
        result.hist_b_delta.max_abs_diff,
        result.hist_b_delta.max_diff_bin
    ));
    out.push_str(&format!(
        "  Luminance: mean_delta={:.3}, stddev_delta={:.3}\n",
        result.luminance_delta.mean_delta, result.luminance_delta.stddev_delta
    ));
    out
}

fn fingerprint_info_to_json(info: &FingerprintInfo) -> Value {
    json!({
        "tool": "get_fingerprint_info",
        "project": info.project_name,
        "file": info.file_path,
        "format": info.file_format,
        "color_space": info.color_space_tag,
        "luminance": {
            "mean": round2(info.luminance_mean),
            "stddev": round2(info.luminance_stddev),
        },
        "histogram_r": histogram_summary_to_json(&info.histogram_r_summary),
        "histogram_g": histogram_summary_to_json(&info.histogram_g_summary),
        "histogram_b": histogram_summary_to_json(&info.histogram_b_summary),
        "tags": info.tags,
    })
}

fn histogram_summary_to_json(summary: &mengxi_core::search::HistogramSummary) -> Value {
    json!({
        "mean_value": round2(summary.mean_value),
        "dominant_bin_range": [summary.dominant_bin_min, summary.dominant_bin_max],
    })
}

fn format_fingerprint_info(info: &FingerprintInfo) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Fingerprint: {}/{}\n",
        info.project_name, info.file_path
    ));
    out.push_str(&format!("  Format: {}\n", info.file_format));
    out.push_str(&format!("  Color space: {}\n", info.color_space_tag));
    out.push_str(&format!(
        "  Luminance: mean={:.2}, stddev={:.2}\n",
        info.luminance_mean, info.luminance_stddev
    ));
    out.push_str(&format!(
        "  R histogram: mean={:.3}, dominant bins {}-{}\n",
        info.histogram_r_summary.mean_value,
        info.histogram_r_summary.dominant_bin_min,
        info.histogram_r_summary.dominant_bin_max
    ));
    out.push_str(&format!(
        "  G histogram: mean={:.3}, dominant bins {}-{}\n",
        info.histogram_g_summary.mean_value,
        info.histogram_g_summary.dominant_bin_min,
        info.histogram_g_summary.dominant_bin_max
    ));
    out.push_str(&format!(
        "  B histogram: mean={:.3}, dominant bins {}-{}\n",
        info.histogram_b_summary.mean_value,
        info.histogram_b_summary.dominant_bin_min,
        info.histogram_b_summary.dominant_bin_max
    ));
    if !info.tags.is_empty() {
        out.push_str(&format!("  Tags: {}\n", info.tags.join(", ")));
    }
    out
}

// --- Error handling ---

fn handle_consistency_error(e: ConsistencyError) -> ToolError {
    match e {
        ConsistencyError::ProjectNotFound(name) => ToolError::ExecutionError(format!(
            "PROJECT_NOT_FOUND -- project '{}' not found",
            name
        )),
        ConsistencyError::NoFingerprints => {
            ToolError::ExecutionError("No fingerprints found in the specified project.".into())
        }
        ConsistencyError::DbError(msg) => {
            ToolError::ExecutionError(format!("DATABASE_ERROR -- {}", msg))
        }
    }
}

fn handle_search_error(e: SearchError) -> ToolError {
    match &e {
        SearchError::ProjectNotFound(name) => ToolError::ExecutionError(format!(
            "PROJECT_NOT_FOUND -- project '{}' not found",
            name
        )),
        SearchError::NoFingerprints => {
            ToolError::ExecutionError("No fingerprint found for the specified project/file.".into())
        }
        SearchError::DatabaseError(msg) => {
            ToolError::ExecutionError(format!("DATABASE_ERROR -- {}", msg))
        }
        _ => ToolError::ExecutionError(e.to_string()),
    }
}
