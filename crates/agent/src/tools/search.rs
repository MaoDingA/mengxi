// tools/search.rs — Search tools (stubs for initial compilation)

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolError, ToolResult};

/// Search by reference image path.
pub struct SearchByImageTool;

#[async_trait]
impl Tool for SearchByImageTool {
    fn name(&self) -> &str { "search_by_image" }
    fn description(&self) -> &str {
        "Search for similar color grading styles using a reference image. Returns ranked results."
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
    async fn execute(&self, _params: Value) -> Result<ToolResult, ToolError> {
        // TODO: Wire to mengxi_core::search::search_by_image
        Ok(ToolResult::err("Search by image not yet wired to core".to_string()))
    }
}

/// Search by tag text.
pub struct SearchByTagTool;

#[async_trait]
impl Tool for SearchByTagTool {
    fn name(&self) -> &str { "search_by_tag" }
    fn description(&self) -> &str {
        "Search for fingerprints by tag text. Returns matching fingerprints ranked by relevance."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tag": { "type": "string", "description": "Tag text to search for" },
                "limit": { "type": "integer", "description": "Max results (default 20)", "default": 20 }
            },
            "required": ["tag"]
        })
    }
    async fn execute(&self, _params: Value) -> Result<ToolResult, ToolError> {
        // TODO: Wire to mengxi_core::search::search_by_tag
        Ok(ToolResult::err("Search by tag not yet wired to core".to_string()))
    }
}

/// Search by color description.
pub struct SearchByColorTool;

#[async_trait]
impl Tool for SearchByColorTool {
    fn name(&self) -> &str { "search_by_color" }
    fn description(&self) -> &str {
        "Search for color grading styles by description. Use when the user describes colors or moods in natural language."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "description": { "type": "string", "description": "Color description" },
                "limit": { "type": "integer", "description": "Max results (default 10)", "default": 10 },
                "project": { "type": "string", "description": "Scope to a specific project name" }
            },
            "required": ["description"]
        })
    }
    async fn execute(&self, _params: Value) -> Result<ToolResult, ToolError> {
        // TODO: Wire to mengxi_core::search::search_histograms
        Ok(ToolResult::err("Search by color not yet wired to core".to_string()))
    }
}
