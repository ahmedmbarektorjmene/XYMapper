//! Configuration storage under `~/.local/share/XXMapper/` with atomic writes.
//!
//! The whole configuration lives in a single `config.json` file. It is never
//! overwritten in place: saves go to a temporary file in the same directory,
//! are flushed and then atomically renamed over the real file so a crash never
//! leaves a half-written configuration behind.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::model::AppConfig;
use crate::error::{AppError, AppResult};

/// `~/.local/share/XXMapper/` following the XDG base directory specification.
pub fn config_dir() -> AppResult<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("HOME")?;
            Some(Path::new(&home).join(".local/share"))
        })
        .ok_or_else(|| AppError::Message("HOME is not set".to_string()))?;

    let dir = base.join("XXMapper");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Path of the configuration file.
pub fn config_file() -> AppResult<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

/// Load and migrate the configuration.
///
/// Missing files are treated as an empty default configuration.
pub fn load_config() -> AppResult<AppConfig> {
    load_config_from(&config_file()?)
}

pub fn load_config_from(path: &Path) -> AppResult<AppConfig> {
    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AppConfig::default());
        }
        Err(e) => return Err(AppError::Io(e)),
    };

    let mut config: AppConfig = serde_json::from_slice(&contents)
        .map_err(|e| AppError::InvalidConfiguration(format!("{}: {e}", path.display())))?;

    config.migrate();
    Ok(config)
}

/// Save the configuration atomically.
pub fn save_config(config: &AppConfig) -> AppResult<()> {
    save_config_to(&config_file()?, config)
}

pub fn save_config_to(path: &Path, config: &AppConfig) -> AppResult<()> {
    let mut serialized = Vec::new();
    serde_json::to_writer_pretty(&mut serialized, config)?;
    serialized.push(b'\n');
    atomic_write(path, &serialized)
}

/// Write `contents` to `path` atomically (temp file + rename).
pub fn atomic_write(path: &Path, contents: &[u8]) -> AppResult<()> {
    let dir = path
        .parent()
        .ok_or_else(|| AppError::InvalidPath(path.to_path_buf()))?;
    fs::create_dir_all(dir)?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");
    let tmp = dir.join(format!(".{file_name}.tmp{}", next_nonce()));

    let result = (|| -> AppResult<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| AppError::ConfigurationWriteFailed(format!("{e}")))?;
        file.write_all(contents)
            .map_err(|e| AppError::ConfigurationWriteFailed(format!("{e}")))?;
        file.sync_all()
            .map_err(|e| AppError::ConfigurationWriteFailed(format!("{e}")))?;
        drop(file);

        fs::rename(&tmp, path).map_err(|e| AppError::ConfigurationWriteFailed(format!("{e}")))?;

        // Make the rename itself durable.
        if let Ok(dir_file) = File::open(dir) {
            let _ = dir_file.sync_all();
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }

    result
}

/// Monotonic nonce for unique temporary file names.
fn next_nonce() -> u64 {
    static NONCE: AtomicU64 = AtomicU64::new(0);
    let n = NONCE.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id() as u64;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    (n << 48) ^ (pid << 24) ^ nanos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::ControllerConfig;
    use crate::controllers::identity::ControllerIdentity;
    use crate::mapping::model::{ControllerMapping, InputSource, Layout};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "xxmapper-test-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("config.json");

        let mut config = AppConfig::default();
        let identity = ControllerIdentity {
            vendor_id: "0079".into(),
            product_id: "0006".into(),
            serial: Some("S1".into()),
            physical_path: Some("usb-path".into()),
            name: "Pad".into(),
        };
        let mut mapping = ControllerMapping::default();
        mapping.a = Some(InputSource::key(304));
        config.controllers.insert(
            identity.id(),
            ControllerConfig {
                identity,
                enabled: true,
                virtual_name: "Arcade Xbox".into(),
                layout: Layout::Ps4,
                mapping,
            },
        );

        save_config_to(&path, &config).unwrap();
        let loaded = load_config_from(&path).unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded, config);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_loads_default() {
        let dir = temp_dir("missing");
        let path = dir.join("config.json");
        let config = load_config_from(&path).unwrap();
        assert_eq!(config.version, 1);
        assert!(config.controllers.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = temp_dir("corrupt");
        let path = dir.join("config.json");
        fs::write(&path, "{ this is not json").unwrap();
        let result = load_config_from(&path);
        assert!(
            result.is_err(),
            "corrupt config must fail, not silently pass"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_write_leaves_no_temp_files() {
        let dir = temp_dir("atomic");
        let path = dir.join("config.json");
        atomic_write(&path, b"one").unwrap();
        atomic_write(&path, b"two").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"two");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files must be cleaned up");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_write_does_not_touch_existing_on_failure() {
        let dir = temp_dir("atomic-fail");
        let path = dir.join("config.json");
        atomic_write(&path, b"original").unwrap();

        // Make the parent a regular file so `create_dir_all` fails reliably
        // even when running as root.
        let blocker = dir.join("not-a-directory");
        fs::write(&blocker, b"x").unwrap();
        let bad_path = blocker.join("config.json");
        let result = atomic_write(&bad_path, b"new");
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_dir_uses_xdg_or_home() {
        // XDG_DATA_HOME wins when set.
        let old_xdg = std::env::var_os("XDG_DATA_HOME");
        let old_home = std::env::var_os("HOME");
        let tmp = temp_dir("env");
        std::env::set_var("XDG_DATA_HOME", &tmp);
        assert_eq!(config_dir().unwrap(), tmp.join("XXMapper"));
        if let Some(old) = old_xdg {
            std::env::set_var("XDG_DATA_HOME", old);
        } else {
            std::env::remove_var("XDG_DATA_HOME");
        }
        if let Some(old) = old_home {
            std::env::set_var("HOME", old);
        } else {
            std::env::remove_var("HOME");
        }
        fs::remove_dir_all(&tmp).ok();
    }
}
