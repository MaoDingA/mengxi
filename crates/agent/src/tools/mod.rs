// tools/mod.rs — Mengxi tool implementations for the agent

mod db_util;
mod search;
mod analyze;
mod project_mgmt;
pub mod hash_anchor;
mod lut_edit;
mod lut_diff;
mod movie_analysis;

pub use search::{
    SearchByColorTool, SearchByImageTool, SearchByTagTool, SearchSimilarRegionTool,
    SearchSimilarTool,
};
pub use analyze::{AnalyzeProjectTool, CompareStylesTool, GetFingerprintInfoTool};
pub use movie_analysis::{
    AnalyzeMovieColorTool, DetectMovieScenesTool, GetMovieMoodTimelineTool, CompareMoviesTool,
};
pub use project_mgmt::{
    ExportLutTool, ImportProjectTool, ListProjectsTool, ReextractFeaturesTool,
};
pub use lut_edit::{
    LoadLutTool, EditLutTool, SaveLutTool, UndoLutEditTool, LutEditStore, new_store as new_lut_edit_store,
};
pub use lut_diff::{DiffLutTool, RenderLutCurvesTool};

use crate::subagent::{SubagentDefinition, SubagentRuntime, SubagentTool};
use crate::tool::ToolRegistry;
use std::path::PathBuf;
use std::sync::Arc;

/// Register all mengxi tools into the registry.
///
/// Creates a shared LutEditStore for LUT editing session management.
/// Returns the store for use in TUI/CLI integration if needed.
pub fn register_all(registry: &mut ToolRegistry) -> LutEditStore {
    let lut_store = new_lut_edit_store();

    // Search tools (Story 3.1)
    registry.register(SearchByImageTool);
    registry.register(SearchByTagTool);
    registry.register(SearchByColorTool);
    registry.register(SearchSimilarTool);
    registry.register(SearchSimilarRegionTool);
    // Analysis tools (Story 3.2)
    registry.register(AnalyzeProjectTool);
    registry.register(CompareStylesTool);
    registry.register(GetFingerprintInfoTool);
    // Project management tools (Story 3.3)
    registry.register(ListProjectsTool);
    registry.register(ImportProjectTool);
    registry.register(ReextractFeaturesTool);
    registry.register(ExportLutTool);
    // LUT editing tools (Story 6.1)
    registry.register(LoadLutTool::new(lut_store.clone()));
    registry.register(EditLutTool::new(lut_store.clone()));
    registry.register(SaveLutTool::new(lut_store.clone()));
    registry.register(UndoLutEditTool::new(lut_store.clone()));
    // LUT diff/curve tools (Story 6.2)
    registry.register(DiffLutTool);
    registry.register(RenderLutCurvesTool);
    // Movie fingerprint analysis tools (Phase 2)
    registry.register(AnalyzeMovieColorTool);
    registry.register(DetectMovieScenesTool);
    registry.register(GetMovieMoodTimelineTool);
    registry.register(CompareMoviesTool);

    lut_store
}

/// Register built-in subagent tools from the `agents/` directory.
///
/// Loads all `.md` subagent definitions from `crates/agent/agents/`,
/// creates a `SubagentTool` for each, and registers them in the registry.
pub fn register_subagents(
    registry: &mut ToolRegistry,
    runtime: Arc<SubagentRuntime>,
) {
    let agents_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents");
    if !agents_dir.exists() {
        return;
    }

    let definitions = SubagentDefinition::load_from_dir(&agents_dir);
    for defn in definitions {
        registry.register(SubagentTool::new(defn, runtime.clone()));
    }
}
