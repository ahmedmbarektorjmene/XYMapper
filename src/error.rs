//! Shared application error type used across all XXMapper modules.

use std::path::PathBuf;

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("controller not found")]
    ControllerNotFound,

    #[error("permission denied accessing input device: {0}")]
    PermissionDenied(String),

    #[error("failed to open evdev device {path}: {source}")]
    EvdevOpenFailed {
        path: String,
        source: std::io::Error,
    },

    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("failed to write configuration: {0}")]
    ConfigurationWriteFailed(String),

    #[error("Xbox backend failed: {0}")]
    XboxBackendFailed(String),

    #[error("uinput backend unavailable: {0}")]
    UinputUnavailable(String),

    #[error("udev error: {0}")]
    Udev(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid path: {0}")]
    InvalidPath(PathBuf),

    #[error("{0}")]
    Message(String),
}
