pub mod bash;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod lsp;
pub mod read;
pub mod task;
pub mod webfetch;
pub mod websearch;
pub mod write;

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
    registry.register(lsp::LspTool::new());
    registry.register(super::planning::PlanTool::new());
    // Register task tracking tools (shared store across all 4 tools)
    task::register_all(registry);
}
