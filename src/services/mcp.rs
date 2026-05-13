#![allow(dead_code)]

use std::collections::HashMap;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

/// MCP (Model Context Protocol) server connection configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub description: String,
}

fn default_true() -> bool {
    true
}

/// A tool discovered from an MCP server
#[derive(Debug, Clone)]
pub struct McpTool {
    pub server_name: String,
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Handle to a running MCP server process
struct McpServerHandle {
    config: McpServerConfig,
    child: Child,
    tools: Vec<McpTool>,
    next_id: u64,
}

impl McpServerHandle {
    /// Send a JSON-RPC request and read the response
    async fn send_request(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        });

        let stdin = self.child.stdin.as_mut().ok_or("stdin not available")?;
        let mut req_str = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        req_str.push('\n');
        stdin.write_all(req_str.as_bytes()).await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;

        // Read response line
        let stdout = self.child.stdout.as_mut().ok_or("stdout not available")?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).await.map_err(|e| e.to_string())?;

        if line.trim().is_empty() {
            return Err("Empty response from MCP server".into());
        }

        let response: serde_json::Value = serde_json::from_str(&line).map_err(|e| format!("Parse error: {}", e))?;

        if let Some(error) = response.get("error") {
            return Err(format!(
                "MCP error: {}",
                error["message"].as_str().unwrap_or("unknown error")
            ));
        }

        Ok(response["result"].clone())
    }

    /// Discover tools from this server
    async fn discover_tools(&mut self) -> Result<Vec<McpTool>, String> {
        let result = self.send_request("tools/list", serde_json::json!({})).await?;

        let tools_json = result["tools"].as_array().ok_or("No tools in response")?;
        let tools: Vec<McpTool> = tools_json
            .iter()
            .map(|t| McpTool {
                server_name: self.config.name.clone(),
                name: t["name"].as_str().unwrap_or("unknown").to_string(),
                description: t["description"].as_str().unwrap_or("").to_string(),
                parameters: t.get("inputSchema").cloned().unwrap_or(serde_json::json!({})),
            })
            .collect();

        Ok(tools)
    }

    /// Call a tool on this server
    async fn call_tool(&mut self, tool_name: &str, arguments: serde_json::Value) -> Result<serde_json::Value, String> {
        self.send_request(
            "tools/call",
            serde_json::json!({
                "name": tool_name,
                "arguments": arguments,
            }),
        )
        .await
    }
}

impl Drop for McpServerHandle {
    fn drop(&mut self) {
        // Attempt to terminate gracefully
        let _ = self.child.start_kill();
    }
}

/// MCP client for communicating with MCP servers
pub struct McpClient {
    configs: Vec<McpServerConfig>,
    connected_servers: HashMap<String, McpServerHandle>,
}

impl McpClient {
    pub fn new() -> Self {
        Self {
            configs: Vec::new(),
            connected_servers: HashMap::new(),
        }
    }

    pub fn add_server(&mut self, config: McpServerConfig) {
        self.configs.push(config);
    }

    pub fn servers(&self) -> &[McpServerConfig] {
        &self.configs
    }

    /// Build a JSON-RPC request (static helper)
    pub fn build_request(method: &str, params: serde_json::Value, id: u64) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        })
    }

    pub fn build_initialize_request(client_name: &str, client_version: &str) -> serde_json::Value {
        Self::build_request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "clientInfo": { "name": client_name, "version": client_version }
            }),
            1,
        )
    }

    pub fn build_list_tools_request() -> serde_json::Value {
        Self::build_request("tools/list", serde_json::json!({}), 2)
    }

    pub fn build_call_tool_request(tool_name: &str, arguments: serde_json::Value) -> serde_json::Value {
        Self::build_request(
            "tools/call",
            serde_json::json!({ "name": tool_name, "arguments": arguments }),
            3,
        )
    }

    pub fn parse_response(response: &serde_json::Value) -> Result<serde_json::Value, String> {
        if let Some(error) = response.get("error") {
            return Err(format!(
                "MCP error: {}",
                error["message"].as_str().unwrap_or("unknown error")
            ));
        }
        Ok(response["result"].clone())
    }

    /// Connect to a configured server and discover its tools
    pub async fn connect(&mut self, server_name: &str) -> Result<Vec<McpTool>, String> {
        let config = self
            .configs
            .iter()
            .find(|c| c.name == server_name && c.enabled)
            .cloned()
            .ok_or_else(|| format!("Server '{}' not found or disabled", server_name))?;

        let mut child = Command::new(&config.command)
            .args(&config.args)
            .envs(&config.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to start {}: {}", config.command, e))?;

        // Initialize the server
        let init_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "clientInfo": { "name": "rustcc", "version": "0.1.0" }
            },
            "id": 0,
        });

        {
            let stdin = child.stdin.as_mut().ok_or("stdin pipe failed")?;
            let mut req = serde_json::to_string(&init_request).map_err(|e| e.to_string())?;
            req.push('\n');
            stdin.write_all(req.as_bytes()).await.map_err(|e| e.to_string())?;
            stdin.flush().await.map_err(|e| e.to_string())?;
        }

        // Read initialize response
        {
            let stdout = child.stdout.as_mut().ok_or("stdout pipe failed")?;
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            reader.read_line(&mut line).await.map_err(|e| e.to_string())?;

            if line.trim().is_empty() {
                return Err("No init response".into());
            }
        }

        let mut handle = McpServerHandle {
            config: config.clone(),
            child,
            tools: Vec::new(),
            next_id: 1,
        };

        // Send initialized notification
        {
            let notif = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {},
            });
            let mut notif_str = serde_json::to_string(&notif).map_err(|e| e.to_string())?;
            notif_str.push('\n');
            let stdin = handle.child.stdin.as_mut().ok_or("stdin pipe failed")?;
            stdin.write_all(notif_str.as_bytes()).await.map_err(|e| e.to_string())?;
            stdin.flush().await.map_err(|e| e.to_string())?;
        }

        // Discover tools
        let tools = handle.discover_tools().await?;
        handle.tools = tools.clone();

        self.connected_servers.insert(server_name.to_string(), handle);

        Ok(tools)
    }

    /// Call a tool on a connected server
    pub async fn call_tool(
        &mut self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let handle = self
            .connected_servers
            .get_mut(server_name)
            .ok_or_else(|| format!("Server '{}' not connected", server_name))?;

        handle.call_tool(tool_name, arguments).await
    }

    /// List all tools from all connected servers
    pub fn all_tools(&self) -> Vec<McpTool> {
        self.connected_servers
            .values()
            .flat_map(|h| h.tools.clone())
            .collect()
    }

    /// Disconnect from a server
    pub fn disconnect(&mut self, server_name: &str) {
        self.connected_servers.remove(server_name);
    }

    /// Check if a server is connected
    pub fn is_connected(&self, server_name: &str) -> bool {
        self.connected_servers.contains_key(server_name)
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

    pub fn add_server(&mut self, config: McpServerConfig) -> Result<(), anyhow::Error> {
        self.client.add_server(config);
        self.save_config()?;
        Ok(())
    }

    pub fn remove_server(&mut self, name: &str) -> Result<(), anyhow::Error> {
        self.client.configs.retain(|c| c.name != name);
        self.client.disconnect(name);
        self.save_config()?;
        Ok(())
    }

    pub fn list_servers(&self) -> &[McpServerConfig] {
        self.client.servers()
    }

    /// Connect to a server and return discovered tools
    pub async fn connect_server(&mut self, name: &str) -> Result<Vec<McpTool>, String> {
        self.client.connect(name).await
    }

    /// Call a tool on a connected server
    pub async fn call_tool(
        &mut self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.client.call_tool(server_name, tool_name, arguments).await
    }

    fn save_config(&self) -> Result<(), anyhow::Error> {
        let json = serde_json::to_string_pretty(&self.client.configs)?;
        std::fs::write(&self.config_path, json)?;
        Ok(())
    }

    pub fn client(&self) -> &McpClient {
        &self.client
    }

    pub fn configs_path(&self) -> String {
        self.config_path.display().to_string()
    }

    /// Get all tools from all connected servers
    pub fn all_tools(&self) -> Vec<McpTool> {
        self.client.all_tools()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request() {
        let req = McpClient::build_request("test", serde_json::json!({"key": "value"}), 1);
        assert_eq!(req["jsonrpc"], "2.0");
        assert_eq!(req["method"], "test");
        assert_eq!(req["id"], 1);
        assert_eq!(req["params"]["key"], "value");
    }

    #[test]
    fn test_parse_response_success() {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "result": {"tools": []},
            "id": 1,
        });
        let result = McpClient::parse_response(&response).unwrap();
        assert_eq!(result["tools"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_parse_response_error() {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "error": {"code": -1, "message": "test error"},
            "id": 1,
        });
        let result = McpClient::parse_response(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("test error"));
    }

    #[test]
    fn test_build_initialize_request() {
        let req = McpClient::build_initialize_request("test-client", "1.0");
        assert_eq!(req["method"], "initialize");
        assert_eq!(req["params"]["clientInfo"]["name"], "test-client");
    }

    #[test]
    fn test_build_list_tools_request() {
        let req = McpClient::build_list_tools_request();
        assert_eq!(req["method"], "tools/list");
    }

    #[test]
    fn test_build_call_tool_request() {
        let req = McpClient::build_call_tool_request("test-tool", serde_json::json!({"arg": 1}));
        assert_eq!(req["method"], "tools/call");
        assert_eq!(req["params"]["name"], "test-tool");
        assert_eq!(req["params"]["arguments"]["arg"], 1);
    }
}
