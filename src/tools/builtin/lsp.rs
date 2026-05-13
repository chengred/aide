use tokio::sync::Mutex;

use async_trait::async_trait;
use serde_json::json;

use crate::services::lsp::LspClient;

use super::super::{Tool, ToolResult};

/// LSP tool providing code intelligence operations.
/// Creates a single LspClient per tool instance (lazy initialization).
pub struct LspTool {
    client: Mutex<Option<LspClient>>,
}

impl LspTool {
    pub fn new() -> Self {
        Self {
            client: Mutex::new(None),
        }
    }
}

#[async_trait]
impl Tool for LspTool {
    fn name(&self) -> &str {
        "lsp"
    }

    fn description(&self) -> &str {
        "Code intelligence via Language Server Protocol. Supports: \
         goToDefinition (find where a symbol is defined), \
         findReferences (find all references), \
         hover (get type/docs for a symbol), \
         documentSymbol (list all symbols in a file)."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["goToDefinition", "findReferences", "hover", "documentSymbol"],
                    "description": "The LSP operation to perform"
                },
                "filePath": {
                    "type": "string",
                    "description": "The file to operate on (absolute or relative path)"
                },
                "line": {
                    "type": "integer",
                    "description": "1-based line number"
                },
                "character": {
                    "type": "integer",
                    "description": "1-based character offset"
                }
            },
            "required": ["operation", "filePath", "line", "character"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> ToolResult {
        let operation = params["operation"].as_str().unwrap_or("");
        let file_path = params["filePath"].as_str().unwrap_or("");
        let line = params["line"].as_u64().unwrap_or(0) as u32;
        let character = params["character"].as_u64().unwrap_or(0) as u32;

        if file_path.is_empty() {
            return ToolResult::err("filePath is required");
        }

        // Convert 1-based to 0-based
        let line = if line > 0 { line - 1 } else { 0 };
        let character = if character > 0 { character - 1 } else { 0 };

        let root = std::env::current_dir()
            .unwrap_or_default()
            .display()
            .to_string();

        let mut guard = self.client.lock().await;
        if guard.is_none() {
            *guard = Some(LspClient::new(&root));
        }
        let client = guard.as_mut().unwrap();

        match operation {
            "goToDefinition" => match client.go_to_definition(file_path, line, character).await {
                Ok(locations) => {
                    if locations.is_empty() {
                        ToolResult::ok("No definition found.")
                    } else {
                        let mut out = String::from("Definitions:\n");
                        for loc in &locations {
                            out.push_str(&format!(
                                "  {} line {} col {}\n",
                                loc.uri,
                                loc.range.start.line + 1,
                                loc.range.start.character + 1,
                            ));
                        }
                        ToolResult::ok(out)
                    }
                }
                Err(e) => ToolResult::err(format!("LSP error: {}", e)),
            },
            "findReferences" => match client.find_references(file_path, line, character).await {
                Ok(locations) => {
                    if locations.is_empty() {
                        ToolResult::ok("No references found.")
                    } else {
                        let mut out = format!("References ({}):\n", locations.len());
                        for loc in &locations {
                            out.push_str(&format!(
                                "  {} line {} col {}\n",
                                loc.uri,
                                loc.range.start.line + 1,
                                loc.range.start.character + 1,
                            ));
                        }
                        ToolResult::ok(out)
                    }
                }
                Err(e) => ToolResult::err(format!("LSP error: {}", e)),
            },
            "hover" => match client.hover(file_path, line, character).await {
                Ok(info) => ToolResult::ok(info.contents),
                Err(e) => ToolResult::err(format!("LSP error: {}", e)),
            },
            "documentSymbol" => match client.document_symbols(file_path).await {
                Ok(symbols) => {
                    if symbols.is_empty() {
                        ToolResult::ok("No symbols found.")
                    } else {
                        let mut out = format!("Symbols in {} ({}):\n", file_path, symbols.len());
                        format_symbols(&mut out, &symbols, 0);
                        ToolResult::ok(out)
                    }
                }
                Err(e) => ToolResult::err(format!("LSP error: {}", e)),
            },
            _ => ToolResult::err(format!("Unknown operation: {}. Use goToDefinition, findReferences, hover, or documentSymbol.", operation)),
        }
    }

    fn requires_approval(&self) -> bool {
        false
    }
}

fn format_symbols(out: &mut String, symbols: &[crate::services::lsp::Symbol], depth: usize) {
    let indent = "  ".repeat(depth);
    for sym in symbols {
        out.push_str(&format!(
            "{}[{}] {} (line {})\n",
            indent,
            sym.kind,
            sym.name,
            sym.range.start.line + 1,
        ));
        if !sym.children.is_empty() {
            format_symbols(out, &sym.children, depth + 1);
        }
    }
}
