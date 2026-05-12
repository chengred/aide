use std::collections::HashSet;
use std::path::Path;

/// Permission decision for a tool execution
#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    /// Always allowed, no confirmation needed
    Allow,
    /// Requires user confirmation before execution
    Confirm,
    /// Blocked entirely
    Deny,
}

/// Manages tool execution permissions
#[derive(Clone)]
pub struct PermissionManager {
    /// Tools that are always allowed
    allowed_tools: HashSet<String>,
    /// Tools that require confirmation
    confirm_tools: HashSet<String>,
    /// Tools that are blocked
    blocked_tools: HashSet<String>,
    /// Allowed paths for file operations (prefix matching)
    allowed_paths: Vec<String>,
    /// Paths excluded from file operations
    excluded_paths: Vec<String>,
    /// Shell commands that are always allowed
    allowed_commands: HashSet<String>,
}

impl PermissionManager {
    pub fn new() -> Self {
        Self {
            allowed_tools: [
                "read".into(),
                "write".into(),
                "edit".into(),
                "grep".into(),
                "glob".into(),
            ]
            .into(),
            confirm_tools: [
                "bash".into(),
                "write".into(),
                "edit".into(),
            ]
            .into(),
            blocked_tools: HashSet::new(),
            allowed_paths: vec![".".into()],
            excluded_paths: vec![
                ".git/".into(),
                ".claude/".into(),
                "node_modules/".into(),
                "target/".into(),
            ],
            allowed_commands: HashSet::new(),
        }
    }

    /// Get the permission decision for a tool with its arguments
    pub fn check(&self, tool_name: &str, params: &serde_json::Value) -> Permission {
        if self.blocked_tools.contains(tool_name) {
            return Permission::Deny;
        }

        // Check path-based permissions for file tools
        if let Some(file_path) = params["file_path"].as_str() {
            if !self.is_path_allowed(file_path) {
                return Permission::Deny;
            }
        }
        if let Some(search_path) = params["path"].as_str() {
            if !self.is_path_allowed(search_path) {
                return Permission::Deny;
            }
        }

        // Check bash command allowlist
        if tool_name == "bash" {
            if let Some(cmd) = params["command"].as_str() {
                if self.is_command_allowed(cmd) {
                    return Permission::Allow;
                }
            }
        }

        if self.confirm_tools.contains(tool_name) {
            return Permission::Confirm;
        }

        if self.allowed_tools.contains(tool_name) {
            return Permission::Allow;
        }

        Permission::Confirm
    }

    /// Allow a tool unconditionally
    pub fn allow_tool(&mut self, tool_name: &str) {
        self.confirm_tools.remove(tool_name);
        self.blocked_tools.remove(tool_name);
        self.allowed_tools.insert(tool_name.to_string());
    }

    /// Block a tool
    #[allow(dead_code)]
    pub fn block_tool(&mut self, tool_name: &str) {
        self.allowed_tools.remove(tool_name);
        self.confirm_tools.remove(tool_name);
        self.blocked_tools.insert(tool_name.to_string());
    }

    /// Add an allowed path
    #[allow(dead_code)]
    pub fn allow_path(&mut self, path: &str) {
        self.allowed_paths.push(path.to_string());
    }

    /// Check if a file path is within allowed paths
    fn is_path_allowed(&self, file_path: &str) -> bool {
        let path = Path::new(file_path);
        // Canonicalize for comparison
        let canonical = if let Ok(c) = path.canonicalize() {
            c
        } else {
            // If path doesn't exist yet, try parent + filename
            match path.parent() {
                Some(parent) if parent.as_os_str().is_empty() => {
                    // No parent — file is in current directory
                    match std::env::current_dir() {
                        Ok(cwd) => cwd.join(path.file_name().unwrap_or_default()),
                        Err(_) => return false,
                    }
                }
                Some(parent) => {
                    match parent.canonicalize() {
                        Ok(parent_canon) => parent_canon.join(path.file_name().unwrap_or_default()),
                        Err(_) => return false,
                    }
                }
                None => return false,
            }
        };

        let canonical_str = canonical.display().to_string();
        let normalized = canonical_str.replace('\\', "/");

        // Check excluded paths
        for excluded in &self.excluded_paths {
            let excl_normalized = excluded.replace('\\', "/");
            if normalized.contains(&excl_normalized) {
                return false;
            }
        }

        // Check allowed paths
        for allowed in &self.allowed_paths {
            let allowed_normalized = if let Ok(c) = Path::new(allowed).canonicalize() {
                c.display().to_string().replace('\\', "/")
            } else {
                allowed.replace('\\', "/")
            };
            if normalized.starts_with(&allowed_normalized) {
                return true;
            }
        }

        false
    }

    /// Check if a shell command is in the allowlist
    fn is_command_allowed(&self, cmd: &str) -> bool {
        let cmd_base = cmd.split_whitespace().next().unwrap_or(cmd);
        self.allowed_commands.contains(cmd_base)
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_tool_allowed() {
        let pm = PermissionManager::new();
        let params = serde_json::json!({"file_path": "src/main.rs"});
        assert_eq!(pm.check("read", &params), Permission::Allow);
    }

    #[test]
    fn test_bash_requires_confirm() {
        let pm = PermissionManager::new();
        let params = serde_json::json!({"command": "ls"});
        assert_eq!(pm.check("bash", &params), Permission::Confirm);
    }

    #[test]
    fn test_unknown_tool_requires_confirm() {
        let pm = PermissionManager::new();
        let params = serde_json::json!({});
        assert_eq!(pm.check("unknown_tool", &params), Permission::Confirm);
    }

    #[test]
    fn test_blocked_tool_denied() {
        let mut pm = PermissionManager::new();
        pm.block_tool("read");
        assert_eq!(pm.check("read", &serde_json::json!({"file_path": "src/main.rs"})), Permission::Deny);
    }

    #[test]
    fn test_allow_tool_removes_confirm() {
        let mut pm = PermissionManager::new();
        pm.allow_tool("bash");
        assert_eq!(pm.check("bash", &serde_json::json!({"command": "ls"})), Permission::Allow);
    }

    #[test]
    fn test_path_outside_allowed_denied() {
        let pm = PermissionManager::new();
        let params = serde_json::json!({"file_path": "/etc/passwd"});
        assert_eq!(pm.check("read", &params), Permission::Deny);
    }

    #[test]
    fn test_excluded_path_denied() {
        let pm = PermissionManager::new();
        let params = serde_json::json!({"file_path": ".git/config"});
        assert_eq!(pm.check("read", &params), Permission::Deny);
    }

    #[test]
    fn test_write_requires_confirm() {
        let pm = PermissionManager::new();
        // Use an absolute path within the current working directory to ensure path check passes
        let cwd = std::env::current_dir().unwrap();
        let file_path = cwd.join("test.txt").display().to_string();
        let params = serde_json::json!({"file_path": file_path, "content": "hello"});
        assert_eq!(pm.check("write", &params), Permission::Confirm);
    }
}

