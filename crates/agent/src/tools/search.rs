// tools/search.rs — Search tools wired to mengxi-core search APIs

use async_trait::async_trait;
use mengxi_core::hybrid_scoring::SignalWeights;
use mengxi_core::search::SearchError;
use mengxi_core::tile_search::TileMode;
use serde_json::{json, Value};
use std::path::Path;

use crate::tool::{Tool, ToolError, ToolResult};
use crate::tools::db_util;

// --- SearchByImageTool ---

pub struct SearchByImageTool;

#[async_trait]
impl Tool for SearchByImageTool {
    fn name(&self) -> &str {
        "search_by_image"
    }
    fn description(&self) -> &str {
        "Search for similar color grading styles using a reference image file. \
         Returns ranked results by visual similarity (CLIP embedding). \
         Falls back to listing all fingerprints if AI is unavailable."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "image_path": { "type": "string", "description": "Path to the reference image file" },
                "limit": { "type": "integer", "description": "Max results (default 10)", "default": 10 },
                "project": { "type": "string", "description": "Scope to a specific project name" }
            },
            "required": ["image_path"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let image_path = params
            .get("image_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: image_path".into()))?;

        if !Path::new(image_path).exists() {
            return Err(ToolError::ExecutionError(format!(
                "FILE_NOT_FOUND -- image file does not exist: {}",
                image_path
            )));
        }

        let conn = db_util::open_connection()?;
        let options = db_util::build_search_options(&params, 10);

        let results = mengxi_core::search::search_by_image(
            &conn,
            image_path,
            &options,
            60,
            120,
            "clip-vit-b-32",
        )
        .map_err(handle_search_error)?;

        let display = db_util::search_results_to_json(&results, self.name());
        let summary = db_util::format_results_summary(display["results"].as_array().unwrap());
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- SearchByTagTool ---

pub struct SearchByTagTool;

#[async_trait]
impl Tool for SearchByTagTool {
    fn name(&self) -> &str {
        "search_by_tag"
    }
    fn description(&self) -> &str {
        "Search for fingerprints by tag text. Supports space-separated multi-tag queries. \
         Returns matching fingerprints ranked by tag match count."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tag": { "type": "string", "description": "Tag text to search for (space-separated for multi-tag)" },
                "limit": { "type": "integer", "description": "Max results (default 20)", "default": 20 },
                "project": { "type": "string", "description": "Scope to a specific project name" }
            },
            "required": ["tag"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let tag = params
            .get("tag")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: tag".into()))?;

        if tag.trim().is_empty() {
            return Err(ToolError::InvalidParams("Parameter 'tag' must not be empty".into()));
        }

        let conn = db_util::open_connection()?;
        let options = db_util::build_search_options(&params, 20);

        let results =
            mengxi_core::search::search_by_tag(&conn, tag, &options).map_err(handle_search_error)?;

        let display = db_util::search_results_to_json(&results, self.name());
        let summary = db_util::format_results_summary(display["results"].as_array().unwrap());
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- SearchByColorTool ---

pub struct SearchByColorTool;

#[async_trait]
impl Tool for SearchByColorTool {
    fn name(&self) -> &str {
        "search_by_color"
    }
    fn description(&self) -> &str {
        "Search for color grading styles by color/mood description. \
         Provide concise tag-like keywords derived from the user's description. \
         Examples: 'warm sunset tones' -> 'warm golden', 'cool sci-fi' -> 'cool blue industrial'. \
         Results are ranked by tag match relevance."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "description": { "type": "string", "description": "Color description translated to keyword tags (e.g., 'warm golden', 'cool blue')" },
                "limit": { "type": "integer", "description": "Max results (default 10)", "default": 10 },
                "project": { "type": "string", "description": "Scope to a specific project name" }
            },
            "required": ["description"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let description = params
            .get("description")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::InvalidParams("Missing required parameter: description".into())
            })?;

        if description.trim().is_empty() {
            return Err(ToolError::InvalidParams(
                "Parameter 'description' must not be empty".into(),
            ));
        }

        let conn = db_util::open_connection()?;
        let options = db_util::build_search_options(&params, 10);

        // Delegate to tag search — description text is used as tag query
        let results = mengxi_core::search::search_by_tag(&conn, description, &options)
            .map_err(handle_search_error)?;

        let display = db_util::search_results_to_json(&results, self.name());
        let results_arr = display["results"].as_array().unwrap();
        let mut summary = format!(
            "Found {} results for color description '{}':\n",
            results_arr.len(),
            description
        );
        summary.push_str(&db_util::format_results_summary(results_arr));
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- SearchSimilarTool (NEW — hybrid search) ---

pub struct SearchSimilarTool;

#[async_trait]
impl Tool for SearchSimilarTool {
    fn name(&self) -> &str {
        "search_similar"
    }
    fn description(&self) -> &str {
        "Search for visually similar fingerprints using multi-signal hybrid scoring. \
         Combines grading features (Oklab histograms), CLIP embeddings, and tags \
         with configurable weights. Use 'grading-first' for color-accurate matching \
         or 'balanced' for general visual similarity."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project name containing the reference file" },
                "file": { "type": "string", "description": "Reference file name within the project" },
                "search_mode": {
                    "type": "string",
                    "enum": ["grading-first", "balanced"],
                    "description": "Preset weight mode (default: grading-first)",
                    "default": "grading-first"
                },
                "weights": {
                    "type": "object",
                    "properties": {
                        "grading": { "type": "number", "description": "Grading signal weight (0.1-1.0)" },
                        "clip": { "type": "number", "description": "CLIP embedding weight (0.1-1.0)" },
                        "tag": { "type": "number", "description": "Tag signal weight (0.1-1.0)" }
                    },
                    "description": "Custom signal weights (must sum to ~1.0). Overrides search_mode."
                },
                "scope_project": { "type": "string", "description": "Scope results to a specific project" },
                "use_pyramid": { "type": "boolean", "description": "Use spatial pyramid matching (default false)", "default": false },
                "limit": { "type": "integer", "description": "Max results (default 10)", "default": 10 }
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
        let file_id = mengxi_core::db::resolve_file_id(&conn, project, file).map_err(
            |_| ToolError::ExecutionError(format!("FILE_NOT_FOUND -- no file '{}/{}'", project, file)),
        )?;

        // Build weights
        let weights = if let Some(w) = params.get("weights").and_then(|v| v.as_object()) {
            let g = w.get("grading").and_then(|v| v.as_f64()).unwrap_or(0.6);
            let c = w.get("clip").and_then(|v| v.as_f64()).unwrap_or(0.3);
            let t = w.get("tag").and_then(|v| v.as_f64()).unwrap_or(0.1);
            SignalWeights { grading: g, clip: c, tag: t }
        } else {
            match params.get("search_mode").and_then(|v| v.as_str()).unwrap_or("grading-first") {
                "balanced" => SignalWeights::balanced(),
                _ => SignalWeights::grading_first(),
            }
        };

        // Build options with scope_project as the project filter
        let mut options = db_util::build_search_options(&params, 10);
        if let Some(scope) = params.get("scope_project").and_then(|v| v.as_str()) {
            options.project = Some(scope.to_string());
        }

        let results = mengxi_core::search::hybrid_search(&conn, file_id, &weights, &options)
            .map_err(handle_search_error)?;

        let display = db_util::hybrid_results_to_json(&results, self.name());
        let summary = db_util::format_results_summary(display["results"].as_array().unwrap());
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- SearchSimilarRegionTool (NEW — tile search) ---

pub struct SearchSimilarRegionTool;

#[async_trait]
impl Tool for SearchSimilarRegionTool {
    fn name(&self) -> &str {
        "search_similar_region"
    }
    fn description(&self) -> &str {
        "Find fingerprints with similar local regions (tiles) to a reference file. \
         Compares per-tile Oklab grading features. Use 'spatial' mode for \
         position-aligned matching or 'any' for position-invariant matching."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project name containing the reference file" },
                "file": { "type": "string", "description": "Reference file name within the project" },
                "mode": {
                    "type": "string",
                    "enum": ["spatial", "any"],
                    "description": "Tile matching mode (default: any)",
                    "default": "any"
                },
                "limit": { "type": "integer", "description": "Max results (default 10)", "default": 10 }
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
        let mode_str = params
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("any");
        let limit = params
            .get("limit")
            .and_then(|v| v.as_i64())
            .map(|v| v as usize)
            .unwrap_or(10);

        let mode = match mode_str {
            "spatial" => TileMode::Spatial,
            "any" => TileMode::Any,
            _ => {
                return Err(ToolError::InvalidParams(format!(
                    "Invalid mode '{}'. Use 'spatial' or 'any'.",
                    mode_str
                )))
            }
        };

        let conn = db_util::open_connection()?;
        let fp_id = mengxi_core::db::resolve_fingerprint_id(&conn, project, file).map_err(
            |_| {
                ToolError::ExecutionError(format!(
                    "FINGERPRINT_NOT_FOUND -- no fingerprint for '{}/{}'",
                    project, file
                ))
            },
        )?;

        let results = mengxi_core::tile_search::tile_search(&conn, fp_id, mode, None, limit)
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

        let display = json!({
            "tool": self.name(),
            "results": results.iter().enumerate().map(|(i, r)| json!({
                "rank": i + 1,
                "project": r.project_name,
                "file": r.file_path,
                "format": r.file_format,
                "score": (r.score * 100.0).round() / 100.0,
                "tile_matches": r.tile_matches.len(),
            })).collect::<Vec<_>>(),
            "total": results.len(),
        });

        let results_arr = display["results"].as_array().unwrap();
        let summary = db_util::format_results_summary(results_arr);
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- Error handling helpers ---

fn handle_search_error(e: SearchError) -> ToolError {
    match &e {
        SearchError::NoFingerprints => {
            // Not an error per se — informational
            // Caller should check via ToolResult::ok
            ToolError::ExecutionError("No indexed fingerprints found in the database.".into())
        }
        SearchError::ProjectNotFound(name) => ToolError::ExecutionError(format!(
            "PROJECT_NOT_FOUND -- project '{}' not found",
            name
        )),
        SearchError::DatabaseError(msg) => {
            ToolError::ExecutionError(format!("DATABASE_ERROR -- {}", msg))
        }
        _ => ToolError::ExecutionError(e.to_string()),
    }
}
