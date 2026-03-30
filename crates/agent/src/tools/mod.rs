// tools/mod.rs — Mengxi tool implementations for the agent

mod db_util;
mod search;
mod analyze;
mod project_mgmt;

pub use search::{
    SearchByColorTool, SearchByImageTool, SearchByTagTool, SearchSimilarRegionTool,
    SearchSimilarTool,
};
pub use analyze::{AnalyzeProjectTool, CompareStylesTool, GetFingerprintInfoTool};
pub use project_mgmt::{
    ExportLutTool, ImportProjectTool, ListProjectsTool, ReextractFeaturesTool,
};

use crate::tool::ToolRegistry;

/// Register all mengxi tools into the registry.
pub fn register_all(registry: &mut ToolRegistry) {
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
}
