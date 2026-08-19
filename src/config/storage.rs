//! Configuration storage under `~/.local/share/XXMapper/` with atomic writes.

use crate::error::{AppError, AppResult};

pub fn config_dir() -> AppResult<std::path::PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("HOME")?;
            Some(std::path::Path::new(&home).join(".local/share"))
        })
        .ok_or_else(|| AppError::Message("HOME is not set".to_string()))?;

    let dir = base.join("XXMapper");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn config_file() -> AppResult<std::path::PathBuf> {
    Ok(config_dir()?.join("config.json"))
}
