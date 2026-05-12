#![allow(dead_code)]

use std::collections::HashMap;

/// MCP (Model Context Protocol) server connection configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServerConfig {
    /// Server name/identifier
    pub name: String,
    /// Command to start the server
    pub command: String,
    /// Arguments for the command
    pub args: Vec<String>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Whether this server is enabled
    pub enabled: bool,
    /// Description of what the server provides
    pub description: String,
}

/// A tool discovered from an MCP server
#[derive(Debug, Clone)]
pub struct McpTool {
    pub server_name: String,
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// MCP client for communicating with MCP servers via JSON-RPC over stdio
pub struct McpClient {
    configs: Vec<McpServerConfig>,
    connected_servers: HashMap<String, McpServerHandle>,
}

struct McpServerHandle {
    config: McpServerConfig,
    // In a full implementation, this would hold:
    // - child process handle
    // - stdin writer
    // - stdout reader
    // - list of discovered tools
    tools: Vec<McpTool>,
}

impl McpClient {
    pub fn new() -> Self {
        Self {
            configs: Vec::new(),
            connected_servers: HashMap::new(),
        }
    }

    /// Add an MCP server configuration
    pub fn add_server(&mut self, config: McpServerConfig) {
        self.configs.push(config);
    }

    /// Get all configured servers
    pub fn servers(&self) -> &[McpServerConfig] {
        &self.configs
    }

    /// JSON-RPC request structure
    pub fn build_request(method: &str, params: serde_json::Value, id: u64) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        })
    }

    /// Build an initialize request for an MCP server
    pub fn build_initialize_request(client_name: &str, client_version: &str) -> serde_json::Value {
        Self::build_request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "clientInfo": {
                    "name": client_name,
                    "version": client_version,
                }
            }),
            1,
        )
    }

    /// Build a tools/list request
    pub fn build_list_tools_request() -> serde_json::Value {
        Self::build_request("tools/list", serde_json::json!({}), 2)
    }

    /// Build a tools/call request
    pub fn build_call_tool_request(tool_name: &str, arguments: serde_json::Value) -> serde_json::Value {
        Self::build_request(
            "tools/call",
            serde_json::json!({
                "name": tool_name,
                "arguments": arguments,
            }),
            3,
        )
    }

    /// Parse a JSON-RPC response
    pub fn parse_response(response: &serde_json::Value) -> Result<serde_json::Value, String> {
        if let Some(error) = response.get("error") {
            return Err(format!(
                "MCP error: {}",
                error["message"].as_str().unwrap_or("unknown error")
            ));
        }
        Ok(response["result"].clone())
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP server manager for configuring and connecting to servers
pub struct McpManager {
    client: McpClient,
    /// Path to the MCP config file
    config_path: std::path::PathBuf,
}

impl McpManager {
    pub fn new() -> Result<Self, anyhow::Error> {
        let dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("config directory not found"))?
            .join("rustcc");
        std::fs::create_dir_all(&dir)?;

        let config_path = dir.join("mcp_servers.json");
        let client = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let servers: Vec<McpServerConfig> = serde_json::from_str(&content).unwrap_or_default();
            let mut client = McpClient::new();
            for server in servers {
                client.add_server(server);
            }
            client
        } else {
            McpClient::new()
        };

        Ok(Self { client, config_path })
    }

    /// Add a server configuration
    pub fn add_server(&mut self, config: McpServerConfig) -> Result<(), anyhow::Error> {
        self.client.add_server(config);
        self.save_config()?;
        Ok(())
    }

    /// Remove a server by name
    pub fn remove_server(&mut self, name: &str) -> Result<(), anyhow::Error> {
        self.client.configs.retain(|c| c.name != name);
        self.save_config()?;
        Ok(())
    }

    /// List configured servers
    pub fn list_servers(&self) -> &[McpServerConfig] {
        self.client.servers()
    }

    /// Save configuration to disk
    fn save_config(&self) -> Result<(), anyhow::Error> {
        let json = serde_json::to_string_pretty(&self.client.configs)?;
        std::fs::write(&self.config_path, json)?;
        Ok(())
    }

    /// Get the MCP client
    pub fn client(&self) -> &McpClient {
        &self.client
    }

    /// Get the MCP config file path
    pub fn configs_path(&self) -> String {
        self.config_path.display().to_string()
    }
}
