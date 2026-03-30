// tools/analyze.rs — Analysis tools

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolError, ToolResult};

/// Analyze a project's color properties.
pub struct AnalyzeProjectTool;

#[async_trait]
impl Tool for AnalyzeProjectTool {
    fn name(&self) -> &str { "analyze_project" }
    fn description(&self) -> &str {
        "Analyze a project's color distribution and statistics."
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
        let _project = params.get("project")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("missing project".to_string()))?;
        // TODO: Wire to mengxi_core
        Ok(ToolResult::err("Analyze project not yet wired to core".to_string()))
    }
}

/// Compare two fingerprint styles.
pub struct CompareStylesTool;

#[async_trait]
impl Tool for CompareStylesTool {
    fn name(&self) -> &str { "compare_styles" }
    fn description(&self) -> &str {
        "Compare two fingerprints side-by-side with detailed feature breakdown."
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
        let _id_a = params.get("id_a").and_then(|v| v.as_i64());
        let _id_b = params.get("id_b").and_then(|v| v.as_i64());
        if _id_a.is_none() || _id_b.is_none() {
            return Err(ToolError::InvalidParams("missing id_a or id_b".to_string()));
        }
        // TODO: Wire to mengxi_core::comparison
        Ok(ToolResult::err("Compare styles not yet wired to core".to_string()))
    }
}

/// Get detailed fingerprint information.
pub struct GetFingerprintInfoTool;

#[async_trait]
impl Tool for GetFingerprintInfoTool {
    fn name(&self) -> &str { "get_fingerprint_info" }
    fn description(&self) -> &str {
        "Get full fingerprint details including color properties and histograms."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project name" },
                "file": { "type": "string", "description": "File path within project" }
            },
            "required": ["project"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let _project = params.get("project")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("missing project".to_string()))?;
        // TODO: Wire to mengxi_core::search::fingerprint_info
        Ok(ToolResult::err("Get fingerprint info not yet wired to core".to_string()))
    }
}
