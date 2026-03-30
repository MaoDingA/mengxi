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
pub use project_mgmt::{ImportProjectTool, ListProjectsTool};

use crate::tool::ToolRegistry;

/// Register all mengxi tools into the registry.
pub fn register_all(registry: &mut ToolRegistry) {
    registry.register(SearchByImageTool);
    registry.register(SearchByTagTool);
    registry.register(SearchByColorTool);
    registry.register(SearchSimilarTool);
    registry.register(SearchSimilarRegionTool);
    registry.register(AnalyzeProjectTool);
    registry.register(CompareStylesTool);
    registry.register(GetFingerprintInfoTool);
    registry.register(ListProjectsTool);
    registry.register(ImportProjectTool);
}
