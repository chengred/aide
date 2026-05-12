pub mod read;
pub mod write;
pub mod edit;
pub mod grep;
pub mod glob;
pub mod bash;
pub mod webfetch;
pub mod websearch;

use super::ToolRegistry;

/// Register all builtin tools into the registry
pub fn register_all(registry: &mut ToolRegistry) {
    registry.register(read::ReadTool::new());
    registry.register(write::WriteTool::new());
    registry.register(edit::EditTool::new());
    registry.register(grep::GrepTool::new());
    registry.register(glob::GlobTool::new());
    registry.register(bash::BashTool::new());
    registry.register(webfetch::WebFetchTool::new());
    registry.register(websearch::WebSearchTool::new());
    registry.register(super::planning::PlanTool::new());
}
