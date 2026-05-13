use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::llm::Message;

/// A saved session record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub provider: String,
    pub model: String,
    pub message_count: usize,
    pub messages: Vec<Message>,
}

/// Session history manager
pub struct HistoryManager {
    sessions_dir: PathBuf,
}

impl HistoryManager {
    pub fn new() -> Result<Self, anyhow::Error> {
        let dir = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("data directory not found"))?
            .join("aide")
            .join("sessions");
        std::fs::create_dir_all(&dir)?;
        Ok(Self { sessions_dir: dir })
    }

    /// Save a session to disk
    #[allow(dead_code)]
    pub fn save(
        &self,
        id: &str,
        title: &str,
        provider: &str,
        model: &str,
        messages: &[Message],
    ) -> Result<PathBuf, anyhow::Error> {
        let record = SessionRecord {
            id: id.to_string(),
            title: title.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            provider: provider.to_string(),
            model: model.to_string(),
            message_count: messages.len(),
            messages: messages.to_vec(),
        };

        let path = self.sessions_dir.join(format!("{}.json", id));
        let json = serde_json::to_string_pretty(&record)?;
        std::fs::write(&path, json)?;

        Ok(path)
    }

    /// Load a session from disk
    pub fn load(&self, id: &str) -> Result<SessionRecord, anyhow::Error> {
        let path = self.sessions_dir.join(format!("{}.json", id));
        if !path.exists() {
            anyhow::bail!("Session not found: {}", id);
        }
        let content = std::fs::read_to_string(&path)?;
        let record: SessionRecord = serde_json::from_str(&content)?;
        Ok(record)
    }

    /// Delete a session
    #[allow(dead_code)]
    pub fn delete(&self, id: &str) -> Result<(), anyhow::Error> {
        let path = self.sessions_dir.join(format!("{}.json", id));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// List all saved sessions (sorted by updated_at, newest first)
    pub fn list(&self) -> Result<Vec<SessionRecord>, anyhow::Error> {
        let mut records = Vec::new();

        let entries = std::fs::read_dir(&self.sessions_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(record) = serde_json::from_str::<SessionRecord>(&content) {
                        records.push(record);
                    }
                }
            }
        }

        records.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(records)
    }

    /// Export a session to a path
    #[allow(dead_code)]
    pub fn export(&self, id: &str, output_path: &str) -> Result<(), anyhow::Error> {
        let record = self.load(id)?;
        let json = serde_json::to_string_pretty(&record)?;
        std::fs::write(output_path, json)?;
        Ok(())
    }

    /// Import a session from a JSON file
    #[allow(dead_code)]
    pub fn import(&self, path: &str) -> Result<SessionRecord, anyhow::Error> {
        let content = std::fs::read_to_string(path)?;
        let record: SessionRecord = serde_json::from_str(&content)?;
        // Save to our sessions dir
        let dest = self.sessions_dir.join(format!("{}.json", record.id));
        std::fs::write(&dest, &content)?;
        Ok(record)
    }

    /// Get the sessions directory
    #[allow(dead_code)]
    pub fn dir(&self) -> &PathBuf {
        &self.sessions_dir
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new().expect("Failed to create history manager")
    }
}
