//! Minimal persisted settings — currently just the last-opened vault path, so the
//! app reopens it on launch.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    pub vault_path: Option<String>,
}

impl Settings {
    pub fn load(path: impl AsRef<Path>) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
