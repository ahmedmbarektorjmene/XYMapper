//! UI-agnostic application state for XXMapper.

use crate::error::AppResult;

pub struct AppState {
    config_dir: std::path::PathBuf,
}

impl AppState {
    pub fn new() -> AppResult<Self> {
        let config_dir = crate::config::storage::config_dir()?;
        Ok(Self { config_dir })
    }

    pub fn config_dir(&self) -> &std::path::Path {
        &self.config_dir
    }
}
