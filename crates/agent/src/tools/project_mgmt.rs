// tools/project_mgmt.rs — Project management tools wired to mengxi-core APIs

use async_trait::async_trait;
use mengxi_core::format_traits::{LutData, LutIo, LutIoError};
use mengxi_core::lut_generation::{ExportLutConfig, LutGenerationError};
use mengxi_core::project::{ImportError, VariantBreakdown};
use mengxi_format::lut as format_lut;
use serde_json::{json, Value};
use std::path::Path;

use crate::project_ops;
use crate::tool::{Tool, ToolError, ToolResult};

/// LUT I/O bridge for the agent layer — delegates to mengxi_format::lut.
struct AgentLutBridge;

impl LutIo for AgentLutBridge {
    fn parse_lut(&self, path: &Path) -> Result<LutData, LutIoError> {
        let data = format_lut::parse_lut(path).map_err(|e| LutIoError::Parse(e.to_string()))?;
        Ok(LutData {
            title: data.title,
            grid_size: data.grid_size,
            domain_min: data.domain_min,
            domain_max: data.domain_max,
            values: data.values,
        })
    }

    fn serialize_lut(&self, data: &LutData, path: &Path) -> Result<(), LutIoError> {
        let inner = format_lut::LutData {
            title: data.title.clone(),
            grid_size: data.grid_size,
            domain_min: data.domain_min,
            domain_max: data.domain_max,
            values: data.values.clone(),
        };
        format_lut::serialize_lut(&inner, path).map_err(|e| LutIoError::Serialize(e.to_string()))
    }
}
use crate::tools::db_util;

// --- ListProjectsTool ---

pub struct ListProjectsTool;

#[async_trait]
impl Tool for ListProjectsTool {
    fn name(&self) -> &str {
        "list_projects"
    }
    fn description(&self) -> &str {
        "List all imported projects with file counts and fingerprint statistics. \
         Returns project name, path, file breakdown (DPX/EXR/MOV), and fingerprint count."
    }
    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }
    async fn execute(&self, _params: Value) -> Result<ToolResult, ToolError> {
        let conn = db_util::open_connection()?;
        let projects =
            mengxi_core::db::db_list_projects(&conn).map_err(|e| {
                ToolError::ExecutionError(format!("DATABASE_ERROR -- {}", e))
            })?;

        let display = json!({
            "tool": "list_projects",
            "projects": projects.iter().map(|p| json!({
                "id": p.id,
                "name": p.name,
                "path": p.path,
                "dpx_count": p.dpx_count,
                "exr_count": p.exr_count,
                "mov_count": p.mov_count,
                "file_count": p.file_count,
                "fingerprint_count": p.fingerprint_count,
            })).collect::<Vec<_>>(),
            "total": projects.len(),
        });

        let mut summary = if projects.is_empty() {
            "No projects imported yet.".to_string()
        } else {
            format!("{} projects:\n", projects.len())
        };
        for p in &projects {
            summary.push_str(&format!(
                "  {} ({} files, {} fingerprints): {}\n",
                p.name, p.file_count, p.fingerprint_count, p.path
            ));
        }
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- ImportProjectTool ---

pub struct ImportProjectTool;

#[async_trait]
impl Tool for ImportProjectTool {
    fn name(&self) -> &str {
        "import_project"
    }
    fn description(&self) -> &str {
        "Import a film project folder for color fingerprinting and indexing. \
         Scans DPX, EXR, and MOV files, extracts color histograms and grading features. \
         If the project already exists, resumes and processes only new files."
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
        let path_str = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: path".into()))?;

        let path = Path::new(path_str);
        if !path.is_dir() {
            return Err(ToolError::ExecutionError(format!(
                "PATH_NOT_FOUND -- '{}' is not a directory",
                path_str
            )));
        }

        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed")
            });

        let conn = db_util::open_connection()?;
        let (_project, breakdown) =
            project_ops::register_project(&conn, name, path, 0, |_, _, _| {})
                .map_err(handle_import_error)?;

        let display = variant_breakdown_to_json(&breakdown, name);
        let summary = format_import_result(name, &breakdown);
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- ReextractFeaturesTool (NEW) ---

pub struct ReextractFeaturesTool;

#[async_trait]
impl Tool for ReextractFeaturesTool {
    fn name(&self) -> &str {
        "reextract_features"
    }
    fn description(&self) -> &str {
        "Re-extract Oklab grading features for fingerprints in a project. \
         Use when fingerprints are marked as 'stale' or after color science updates. \
         Processes all fingerprints in the specified project."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project name to re-extract" }
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

        let fps = project_ops::list_fingerprints_by_project(&conn, project)
            .map_err(|e| ToolError::ExecutionError(format!("DATABASE_ERROR -- {}", e)))?;

        if fps.is_empty() {
            return Ok(ToolResult::ok(format!(
                "No fingerprints found for project '{}'.",
                project
            )));
        }

        let result = project_ops::batch_reextract_grading_features(
            &conn,
            &fps,
            0, // tile_grid_size=0: skip tile extraction
            |_, _, _| {},
        )
        .map_err(|e| ToolError::ExecutionError(format!("REEXTRACT_ERROR -- {}", e)))?;

        let display = json!({
            "tool": "reextract_features",
            "project": project,
            "total": fps.len(),
            "reextracted": result.reextracted,
            "skipped": result.skipped,
            "failed": result.failed,
            "failures": result.failures.iter().map(|(f, r)| json!({
                "file": f,
                "reason": r,
            })).collect::<Vec<_>>(),
        });

        let summary = format!(
            "Re-extracted {}/{} fingerprints for '{}' ({} skipped, {} failed)",
            result.reextracted, fps.len(), project, result.skipped, result.failed
        );
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- ExportLutTool (NEW) ---

pub struct ExportLutTool;

#[async_trait]
impl Tool for ExportLutTool {
    fn name(&self) -> &str {
        "export_lut"
    }
    fn description(&self) -> &str {
        "Export a LUT (Look-Up Table) file for a project's color transform. \
         Supports cube, 3dl, look, and csp formats. \
         Generates the LUT from the project's fingerprint data."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project name" },
                "format": {
                    "type": "string",
                    "enum": ["cube", "3dl", "look", "csp"],
                    "description": "LUT format (default: cube)",
                    "default": "cube"
                },
                "output": { "type": "string", "description": "Output file path" },
                "grid_size": { "type": "integer", "description": "LUT grid size (default: 33)", "default": 33 }
            },
            "required": ["project", "output"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let project_name = params
            .get("project")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: project".into()))?;
        let output = params
            .get("output")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: output".into()))?;
        let format = params
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("cube");
        let grid_size = params
            .get("grid_size")
            .and_then(|v| v.as_i64())
            .unwrap_or(33) as u32;

        let conn = db_util::open_connection()?;

        // Resolve project ID from name
        let project = mengxi_core::project::get_project(&conn, project_name)
            .map_err(|e| ToolError::ExecutionError(format!("DATABASE_ERROR -- {}", e)))?
            .ok_or_else(|| {
                ToolError::ExecutionError(format!(
                    "PROJECT_NOT_FOUND -- project '{}' not found",
                    project_name
                ))
            })?;

        let mut config =
            ExportLutConfig::new(project.id, output.into(), format.to_string());
        config.grid_size = grid_size;

        let result =
            mengxi_core::lut_generation::export_lut_force(&conn, &AgentLutBridge, config).map_err(|e| match e {
                LutGenerationError::FingerprintNotFound => {
                    ToolError::ExecutionError("FINGERPRINT_NOT_FOUND -- no fingerprints for this project".into())
                }
                LutGenerationError::OverwriteDenied(p) => {
                    ToolError::ExecutionError(format!("FILE_EXISTS -- {}", p.display()))
                }
                LutGenerationError::WriteError(msg) => {
                    ToolError::ExecutionError(format!("WRITE_ERROR -- {}", msg))
                }
                _ => ToolError::ExecutionError(e.to_string()),
            })?;

        let display = json!({
            "tool": "export_lut",
            "path": result.path.to_string_lossy(),
            "format": result.format,
            "grid_size": result.grid_size,
        });
        let summary = format!(
            "Exported {} LUT ({}x{}) to {}",
            result.format, result.grid_size, result.grid_size, result.path.display()
        );
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- Helpers ---

fn variant_breakdown_to_json(breakdown: &VariantBreakdown, name: &str) -> Value {
    json!({
        "tool": "import_project",
        "project": name,
        "fingerprint_count": breakdown.fingerprint_count,
        "grading_feature_count": breakdown.grading_feature_count,
        "variants": breakdown.variants,
        "skipped_count": breakdown.skipped_count,
        "skipped_files": breakdown.skipped_files,
        "resumed_count": breakdown.resumed_count,
    })
}

fn format_import_result(name: &str, breakdown: &VariantBreakdown) -> String {
    let mut out = format!("Imported project '{}'\n", name);
    out.push_str(&format!(
        "  {} fingerprints extracted ({} with grading features)\n",
        breakdown.fingerprint_count, breakdown.grading_feature_count
    ));
    if !breakdown.variants.is_empty() {
        out.push_str(&format!("  Variants: {}\n", breakdown.variants.join(", ")));
    }
    if breakdown.skipped_count > 0 {
        out.push_str(&format!("  {} files skipped\n", breakdown.skipped_count));
    }
    if breakdown.resumed_count > 0 {
        out.push_str(&format!(
            "  {} files resumed from previous import\n",
            breakdown.resumed_count
        ));
    }
    out
}

fn handle_import_error(e: ImportError) -> ToolError {
    match e {
        ImportError::PathNotFound(p) => {
            ToolError::ExecutionError(format!("PATH_NOT_FOUND -- {}", p))
        }
        ImportError::DuplicateName(name) => {
            // This shouldn't happen (register_project handles resume), but just in case
            ToolError::ExecutionError(format!(
                "DUPLICATE_NAME -- project '{}' already exists",
                name
            ))
        }
        ImportError::DbError(msg) => {
            ToolError::ExecutionError(format!("DATABASE_ERROR -- {}", msg))
        }
        ImportError::CorruptFile { filename, reason } => {
            ToolError::ExecutionError(format!(
                "CORRUPT_FILE -- {}: {}",
                filename, reason
            ))
        }
    }
}
