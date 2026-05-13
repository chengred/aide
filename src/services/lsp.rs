//! Lightweight LSP client for code intelligence operations.
//! Communicates with language servers via stdio using the Language Server Protocol.

use std::collections::HashMap;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

/// Configuration for a language server
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LspServerConfig {
    pub language: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub file_extensions: Vec<String>,
}

/// A symbol returned by documentSymbol
#[derive(Debug, Clone, serde::Serialize)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub range: LspRange,
    pub children: Vec<Symbol>,
}

/// A location in a file
#[derive(Debug, Clone, serde::Serialize)]
pub struct LspLocation {
    pub uri: String,
    pub range: LspRange,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

/// Hover information
#[derive(Debug, Clone, serde::Serialize)]
pub struct HoverInfo {
    pub contents: String,
    pub range: Option<LspRange>,
}

/// Handle to a running LSP server
struct LspHandle {
    language: String,
    child: Child,
    next_id: u64,
}

impl LspHandle {
    async fn send_request(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        });

        let req_str = format!("Content-Length: {}\r\n\r\n{}",
            serde_json::to_string(&request).map_err(|e| e.to_string())?.len(),
            serde_json::to_string(&request).map_err(|e| e.to_string())?
        );

        let stdin = self.child.stdin.as_mut().ok_or("stdin not available")?;
        stdin.write_all(req_str.as_bytes()).await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;

        // Read LSP header
        let stdout = self.child.stdout.as_mut().ok_or("stdout not available")?;
        let mut reader = BufReader::new(stdout);
        let mut header = String::new();

        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await.map_err(|e| e.to_string())?;
            if line == "\r\n" || line == "\n" || line.is_empty() {
                break;
            }
            header.push_str(&line);
        }

        // Parse Content-Length
        let content_length: usize = header
            .lines()
            .find(|l| l.to_lowercase().starts_with("content-length:"))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|v| v.trim().parse().ok())
            .ok_or("Missing Content-Length header")?;

        // Read body
        let mut body = vec![0u8; content_length];
        tokio::io::AsyncReadExt::read_exact(reader.get_mut(), &mut body)
            .await
            .map_err(|e| e.to_string())?;

        let response: serde_json::Value =
            serde_json::from_slice(&body).map_err(|e| format!("Parse error: {}", e))?;

        if let Some(error) = response.get("error") {
            return Err(error["message"].as_str().unwrap_or("LSP error").to_string());
        }

        Ok(response["result"].clone())
    }

    async fn initialize(&mut self, root_uri: &str) -> Result<(), String> {
        let result = self.send_request("initialize", serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "definition": { "dynamicRegistration": true },
                    "references": { "dynamicRegistration": true },
                    "hover": {
                        "dynamicRegistration": true,
                        "contentFormat": ["plaintext", "markdown"]
                    },
                    "documentSymbol": {
                        "dynamicRegistration": true,
                        "symbolKind": { "valueSet": [] }
                    }
                }
            },
            "workspaceFolders": [{"uri": root_uri, "name": "workspace"}]
        })).await?;

        // Send initialized notification
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        let notif_str = format!("Content-Length: {}\r\n\r\n{}",
            serde_json::to_string(&notif).map_err(|e| e.to_string())?.len(),
            serde_json::to_string(&notif).map_err(|e| e.to_string())?
        );
        let stdin = self.child.stdin.as_mut().ok_or("stdin gone")?;
        stdin.write_all(notif_str.as_bytes()).await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;

        let _ = result;
        Ok(())
    }

    async fn did_open(&mut self, file_path: &str, content: &str) -> Result<(), String> {
        let uri = format!("file:///{}", file_path.replace('\\', "/"));
        let _ = self.send_request("textDocument/didOpen", serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": self.language,
                "version": 1,
                "text": content
            }
        })).await?;
        // Give server a moment to index
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        Ok(())
    }
}

impl Drop for LspHandle {
    fn drop(&mut self) {
        // Send shutdown
        let shutdown = serde_json::json!({
            "jsonrpc": "2.0", "method": "shutdown", "id": 9999
        });
        let s = serde_json::to_string(&shutdown).unwrap_or_default();
        let _ = std::process::Command::new("echo").arg(s).output();

        let _ = self.child.start_kill();
    }
}

/// Main LSP client managing connections to language servers
pub struct LspClient {
    handles: HashMap<String, LspHandle>,
    root_path: String,
}

impl LspClient {
    pub fn new(root_path: &str) -> Self {
        Self {
            handles: HashMap::new(),
            root_path: root_path.to_string(),
        }
    }

    /// Get or start a language server for the given file extension
    async fn get_handle(&mut self, language: &str, _file_ext: &str) -> Result<&mut LspHandle, String> {
        if !self.handles.contains_key(language) {
            let config = match language {
                "rust" => LspServerConfig {
                    language: "rust".into(),
                    command: "rust-analyzer".into(),
                    args: vec![],
                    file_extensions: vec!["rs".into()],
                },
                "python" => LspServerConfig {
                    language: "python".into(),
                    command: "pyright-langserver".into(),
                    args: vec!["--stdio".into()],
                    file_extensions: vec!["py".into()],
                },
                "typescript" | "javascript" => LspServerConfig {
                    language: "typescript".into(),
                    command: "typescript-language-server".into(),
                    args: vec!["--stdio".into()],
                    file_extensions: vec!["ts".into(), "tsx".into(), "js".into(), "jsx".into()],
                },
                "go" => LspServerConfig {
                    language: "go".into(),
                    command: "gopls".into(),
                    args: vec![],
                    file_extensions: vec!["go".into()],
                },
                _ => return Err(format!("No LSP server configured for language: {}", language)),
            };

            let child = Command::new(&config.command)
                .args(&config.args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| format!("Failed to start {}: {}", config.command, e))?;

            let root_uri = format!("file:///{}", self.root_path.replace('\\', "/"));
            let mut handle = LspHandle {
                language: config.language.clone(),
                child,
                next_id: 1,
            };

            handle.initialize(&root_uri).await?;
            self.handles.insert(language.to_string(), handle);
        }

        self.handles.get_mut(language).ok_or_else(|| "Server handle lost".into())
    }

    /// Open a file in the LSP server
    async fn ensure_open(&mut self, file_path: &str) -> Result<String, String> {
        let ext = std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let language = ext_to_language(ext);

        let content = std::fs::read_to_string(file_path).map_err(|e| e.to_string())?;
        let handle = self.get_handle(language, ext).await?;
        handle.did_open(file_path, &content).await?;

        Ok(language.to_string())
    }

    /// Go to definition
    pub async fn go_to_definition(&mut self, file_path: &str, line: u32, character: u32) -> Result<Vec<LspLocation>, String> {
        self.ensure_open(file_path).await?;
        let ext = std::path::Path::new(file_path).extension().and_then(|e| e.to_str()).unwrap_or("");
        let language = ext_to_language(ext);
        let handle = self.handles.get_mut(language).ok_or("Server not connected")?;

        let uri = format!("file:///{}", file_path.replace('\\', "/"));
        let result = handle.send_request("textDocument/definition", serde_json::json!({
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character}
        })).await?;

        parse_locations(&result)
    }

    /// Find references
    pub async fn find_references(&mut self, file_path: &str, line: u32, character: u32) -> Result<Vec<LspLocation>, String> {
        self.ensure_open(file_path).await?;
        let ext = std::path::Path::new(file_path).extension().and_then(|e| e.to_str()).unwrap_or("");
        let language = ext_to_language(ext);
        let handle = self.handles.get_mut(language).ok_or("Server not connected")?;

        let uri = format!("file:///{}", file_path.replace('\\', "/"));
        let result = handle.send_request("textDocument/references", serde_json::json!({
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character},
            "context": {"includeDeclaration": true}
        })).await?;

        parse_locations(&result)
    }

    /// Get hover information
    pub async fn hover(&mut self, file_path: &str, line: u32, character: u32) -> Result<HoverInfo, String> {
        self.ensure_open(file_path).await?;
        let ext = std::path::Path::new(file_path).extension().and_then(|e| e.to_str()).unwrap_or("");
        let language = ext_to_language(ext);
        let handle = self.handles.get_mut(language).ok_or("Server not connected")?;

        let uri = format!("file:///{}", file_path.replace('\\', "/"));
        let result = handle.send_request("textDocument/hover", serde_json::json!({
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": character}
        })).await?;

        let contents = if let Some(s) = result["contents"].as_str() {
            s.to_string()
        } else if let Some(arr) = result["contents"].as_array() {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        } else if let Some(obj) = result["contents"].as_object() {
            obj.get("value").and_then(|v| v.as_str()).unwrap_or("(no hover info)").to_string()
        } else {
            "(no hover info)".to_string()
        };

        Ok(HoverInfo {
            contents,
            range: None,
        })
    }

    /// Get document symbols
    pub async fn document_symbols(&mut self, file_path: &str) -> Result<Vec<Symbol>, String> {
        self.ensure_open(file_path).await?;
        let ext = std::path::Path::new(file_path).extension().and_then(|e| e.to_str()).unwrap_or("");
        let language = ext_to_language(ext);
        let handle = self.handles.get_mut(language).ok_or("Server not connected")?;

        let uri = format!("file:///{}", file_path.replace('\\', "/"));
        let result = handle.send_request("textDocument/documentSymbol", serde_json::json!({
            "textDocument": {"uri": uri}
        })).await?;

        parse_symbols(&result)
    }
}

fn ext_to_language(ext: &str) -> &str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" => "cpp",
        _ => ext,
    }
}

fn parse_locations(result: &serde_json::Value) -> Result<Vec<LspLocation>, String> {
    if result.is_null() {
        return Ok(Vec::new());
    }

    // Can be a single location or array
    let locations: Vec<&serde_json::Value> = if result.is_array() {
        result.as_array().unwrap().iter().collect()
    } else if result.is_object() {
        vec![result]
    } else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for loc in locations {
        let uri = loc["uri"].as_str().unwrap_or("").to_string();
        if let (Some(start), Some(end)) = (parse_position(&loc["range"]["start"]), parse_position(&loc["range"]["end"])) {
            out.push(LspLocation {
                uri,
                range: LspRange { start, end },
            });
        }
    }
    Ok(out)
}

fn parse_position(v: &serde_json::Value) -> Option<LspPosition> {
    Some(LspPosition {
        line: v["line"].as_u64()? as u32,
        character: v["character"].as_u64()? as u32,
    })
}

fn parse_symbols(result: &serde_json::Value) -> Result<Vec<Symbol>, String> {
    let arr = result.as_array().ok_or("Expected array of symbols")?;
    let symbols: Vec<Symbol> = arr.iter().map(|s| parse_symbol(s)).collect();
    Ok(symbols)
}

fn parse_symbol(v: &serde_json::Value) -> Symbol {
    let kind = match v["kind"].as_u64().unwrap_or(0) {
        1 => "file", 2 => "module", 3 => "namespace", 4 => "package",
        5 => "class", 6 => "method", 7 => "property", 8 => "field",
        9 => "constructor", 10 => "enum", 11 => "interface", 12 => "function",
        13 => "variable", 14 => "constant", 15 => "string", 16 => "number",
        17 => "boolean", 18 => "array", 19 => "object", 20 => "key", _ => "unknown",
    };
    Symbol {
        name: v["name"].as_str().unwrap_or("?").to_string(),
        kind: kind.to_string(),
        range: LspRange {
            start: parse_position(&v["range"]["start"]).unwrap_or(LspPosition { line: 0, character: 0 }),
            end: parse_position(&v["range"]["end"]).unwrap_or(LspPosition { line: 0, character: 0 }),
        },
        children: v["children"].as_array().map(|a| a.iter().map(parse_symbol).collect()).unwrap_or_default(),
    }
}
