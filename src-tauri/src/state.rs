//! Shared application state: the currently open vault + its index, and where to
//! persist settings.

use crate::index::Index;
use crate::settings::Settings;
use crate::vault::Vault;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct OpenVault {
    pub vault: Vault,
    pub index: Index,
}

pub struct AppState {
    pub open: Mutex<Option<OpenVault>>,
    pub settings_path: PathBuf,
}

impl AppState {
    pub fn new(settings_path: PathBuf) -> Self {
        Self {
            open: Mutex::new(None),
            settings_path,
        }
    }

    pub fn settings(&self) -> Settings {
        Settings::load(&self.settings_path)
    }

    /// Open a folder as the active vault: build/sync the index and remember the path.
    pub fn open_vault(&self, path: &str) -> Result<()> {
        let vault = Vault::open(path)?;
        let index_dir = vault.root().join(".local-roam");
        std::fs::create_dir_all(&index_dir)?;
        let index = Index::open(index_dir.join("index.sqlite"))?;
        index.rebuild_from_vault(&vault)?;

        *self.open.lock().unwrap() = Some(OpenVault { vault, index });

        let mut settings = self.settings();
        settings.vault_path = Some(path.to_string());
        settings.save(&self.settings_path)?;
        Ok(())
    }
}
