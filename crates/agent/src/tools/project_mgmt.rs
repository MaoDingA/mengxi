// tools/project_mgmt.rs — Project management tools

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolError, ToolResult};

/// List all projects.
pub struct ListProjectsTool;

#[async_trait]
impl Tool for ListProjectsTool {
    fn name(&self) -> &str { "list_projects" }
    fn description(&self) -> &str {
        "List all imported projects with file counts and basic statistics."
    }
    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }
    async fn execute(&self, _params: Value) -> Result<ToolResult, ToolError> {
        // TODO: Wire to mengxi_core::project::list_projects
        Ok(ToolResult::err("List projects not yet wired to core".to_string()))
    }
}

/// Import a project from a path.
pub struct ImportProjectTool;

#[async_trait]
impl Tool for ImportProjectTool {
    fn name(&self) -> &str { "import_project" }
    fn description(&self) -> &str {
        "Import a film project folder for fingerprinting and indexing."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the project folder" },
                "name": { "type": "string", "description": "Project name (defaults to folder name)" }
            },
            "required": ["path"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let _path = params.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("missing path".to_string()))?;
        // TODO: Wire to mengxi_core::project::register_project
        Ok(ToolResult::err("Import project not yet wired to core".to_string()))
    }
}
