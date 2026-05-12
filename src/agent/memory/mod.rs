#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Memory types (same categories as Claude Code) ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MemoryType {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "feedback")]
    Feedback,
    #[serde(rename = "project")]
    Project,
    #[serde(rename = "reference")]
    Reference,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::User => write!(f, "user"),
            MemoryType::Feedback => write!(f, "feedback"),
            MemoryType::Project => write!(f, "project"),
            MemoryType::Reference => write!(f, "reference"),
        }
    }
}

impl std::str::FromStr for MemoryType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user" => Ok(MemoryType::User),
            "feedback" => Ok(MemoryType::Feedback),
            "project" => Ok(MemoryType::Project),
            "reference" => Ok(MemoryType::Reference),
            _ => Err(format!("Unknown memory type: {}", s)),
        }
    }
}

// ── Memory entry ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

// ── File-based persistent memory store ──

#[derive(Clone)]
pub struct MemoryStore {
    base_dir: PathBuf,
    /// In-memory cache of loaded entries
    entries: Vec<MemoryEntry>,
    /// MEMORY.md index entries
    index_entries: Vec<String>,
}

impl MemoryStore {
    /// Open the memory store from the default directory
    pub fn open() -> Result<Self, anyhow::Error> {
        let base_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("config directory not found"))?
            .join("rustcc")
            .join("memory");

        std::fs::create_dir_all(&base_dir)?;

        let mut store = Self {
            base_dir,
            entries: Vec::new(),
            index_entries: Vec::new(),
        };

        store.load_index()?;
        Ok(store)
    }

    /// Open a project-specific memory store
    pub fn open_project(project_dir: &str) -> Result<Self, anyhow::Error> {
        let base_dir = PathBuf::from(project_dir).join(".claude").join("memory");
        std::fs::create_dir_all(&base_dir)?;

        let mut store = Self {
            base_dir,
            entries: Vec::new(),
            index_entries: Vec::new(),
        };

        store.load_index()?;
        Ok(store)
    }

    /// Load MEMORY.md index
    fn load_index(&mut self) -> Result<(), anyhow::Error> {
        let index_path = self.base_dir.join("MEMORY.md");
        if index_path.exists() {
            let content = std::fs::read_to_string(&index_path)?;
            self.index_entries = content.lines().map(|s| s.to_string()).collect();
        }
        Ok(())
    }

    /// Load all memory entries from disk
    pub fn load_all(&mut self) -> Result<(), anyhow::Error> {
        self.entries.clear();
        let entries = std::fs::read_dir(&self.base_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "md") && path.file_name().map_or(false, |n| n != "MEMORY.md") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(memory) = Self::parse_frontmatter(&content, &path) {
                        self.entries.push(memory);
                    }
                }
            }
        }
        Ok(())
    }

    /// Parse a memory file with YAML-like frontmatter
    fn parse_frontmatter(content: &str, path: &std::path::Path) -> Option<MemoryEntry> {
        let content = content.trim();
        if !content.starts_with("---") {
            return None;
        }

        let rest = &content[3..];
        let end = rest.find("---")?;
        let frontmatter = &rest[..end];
        let body = rest[end + 3..].trim();

        let mut name = String::new();
        let mut description = String::new();
        let mut memory_type = MemoryType::Project;

        for line in frontmatter.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim();
                match key.trim().to_lowercase().as_str() {
                    "name" => name = value.to_string(),
                    "description" => description = value.to_string(),
                    "type" => memory_type = value.parse().unwrap_or(MemoryType::Project),
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            name = path.file_stem()?.to_str()?.to_string();
        }

        Some(MemoryEntry {
            name,
            description,
            memory_type,
            content: body.to_string(),
            created_at: String::new(),
            updated_at: String::new(),
        })
    }

    /// Save a memory entry to disk
    pub fn save(
        &mut self,
        name: &str,
        description: &str,
        memory_type: MemoryType,
        content: &str,
    ) -> Result<(), anyhow::Error> {
        let now = chrono::Utc::now().to_rfc3339();

        let frontmatter = format!(
            "---\nname: {}\ndescription: {}\ntype: {}\n---\n\n{}",
            name, description, memory_type.to_string(), content
        );

        // Sanitize filename
        let filename = name
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "");
        let path = self.base_dir.join(format!("{}.md", filename));
        std::fs::write(&path, frontmatter)?;

        // Update or add index entry
        let index_line = format!("- [{}]({}.md) — {}", name, filename, description);
        self.update_index(&index_line)?;

        // Update in-memory cache
        self.entries.retain(|e| e.name != name);
        self.entries.push(MemoryEntry {
            name: name.to_string(),
            description: description.to_string(),
            memory_type,
            content: content.to_string(),
            created_at: now.clone(),
            updated_at: now,
        });

        Ok(())
    }

    /// Delete a memory entry
    pub fn delete(&mut self, name: &str) -> Result<(), anyhow::Error> {
        let filename = name
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "");
        let path = self.base_dir.join(format!("{}.md", filename));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.entries.retain(|e| e.name != name);
        self.rebuild_index()?;
        Ok(())
    }

    /// Update MEMORY.md index
    fn update_index(&mut self, entry: &str) -> Result<(), anyhow::Error> {
        // Replace existing entry or append
        let entry_name = entry.split("](").next().unwrap_or(entry);
        self.index_entries.retain(|e| !e.starts_with(entry_name));
        self.index_entries.push(entry.to_string());
        self.write_index()?;
        Ok(())
    }

    /// Rebuild the entire index from entries
    fn rebuild_index(&self) -> Result<(), anyhow::Error> {
        let lines: Vec<String> = self
            .entries
            .iter()
            .map(|e| {
                let filename = e.name.to_lowercase().replace(' ', "_");
                format!("- [{}]({}.md) — {}", e.name, filename, e.description)
            })
            .collect();
        let index_path = self.base_dir.join("MEMORY.md");
        std::fs::write(&index_path, lines.join("\n"))?;
        Ok(())
    }

    fn write_index(&self) -> Result<(), anyhow::Error> {
        let index_path = self.base_dir.join("MEMORY.md");
        std::fs::write(&index_path, self.index_entries.join("\n"))?;
        Ok(())
    }

    /// Search memory by keyword (case-insensitive)
    pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
        let lower = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.name.to_lowercase().contains(&lower)
                    || e.content.to_lowercase().contains(&lower)
                    || e.description.to_lowercase().contains(&lower)
            })
            .collect()
    }

    /// Get entries by type
    pub fn by_type(&self, t: &MemoryType) -> Vec<&MemoryEntry> {
        self.entries.iter().filter(|e| &e.memory_type == t).collect()
    }

    /// Build a context string to inject into the system prompt
    pub fn build_context_string(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut ctx = String::from("\n\n[Relevant memories from past sessions]\n");
        for entry in &self.entries {
            ctx.push_str(&format!(
                "- [{}] ({}) {}: {}\n",
                entry.memory_type.to_string(),
                entry.name,
                entry.description,
                entry.content.chars().take(200).collect::<String>()
            ));
        }
        ctx
    }

    /// Number of loaded entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Get all entries
    pub fn entries(&self) -> &[MemoryEntry] {
        &self.entries
    }

    /// Get the base directory
    pub fn dir(&self) -> &PathBuf {
        &self.base_dir
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::open().unwrap_or_else(|_| {
            // Fallback: create in temp dir
            let dir = std::env::temp_dir().join("rustcc_memory");
            std::fs::create_dir_all(&dir).ok();
            Self {
                base_dir: dir,
                entries: Vec::new(),
                index_entries: Vec::new(),
            }
        })
    }
}
