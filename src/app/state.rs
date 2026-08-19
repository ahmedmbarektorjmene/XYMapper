//! UI-agnostic application state for XXMapper.

use std::path::{Path, PathBuf};

use crate::config::model::{AppConfig, ControllerConfig};
use crate::config::storage;
use crate::controllers::discovery::DiscoveredController;
use crate::error::AppResult;
use crate::mapping::model::{ControllerMapping, Layout};

/// Application state shared by the UI and the mapping session.
pub struct AppState {
    config_dir: PathBuf,
    config: AppConfig,
}

impl AppState {
    pub fn new() -> AppResult<Self> {
        let config_dir = storage::config_dir()?;
        let config = storage::load_config()?;
        Ok(Self { config_dir, config })
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut AppConfig {
        &mut self.config
    }

    /// The saved configuration for a discovered controller, if any.
    pub fn controller_for(&self, discovered: &DiscoveredController) -> Option<&ControllerConfig> {
        self.config.controllers.get(&discovered.identity.id())
    }

    /// Get or create the configuration entry for a discovered controller and
    /// apply `f` to it.
    pub fn update_controller<F: FnOnce(&mut ControllerConfig)>(
        &mut self,
        discovered: &DiscoveredController,
        f: F,
    ) {
        let identity = discovered.identity.clone();
        let id = identity.id();
        let entry = self
            .config
            .controllers
            .entry(id)
            .or_insert_with(|| ControllerConfig {
                identity,
                enabled: false,
                virtual_name: String::new(),
                layout: Layout::Custom,
                mapping: ControllerMapping::default(),
            });
        f(entry);
    }

    /// Replace the typed mapping of a discovered controller.
    pub fn set_mapping(&mut self, discovered: &DiscoveredController, mapping: ControllerMapping) {
        self.update_controller(discovered, |entry| entry.mapping = mapping);
    }

    /// Persist the configuration to disk.
    pub fn save(&self) -> AppResult<()> {
        storage::save_config(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controllers::identity::ControllerIdentity;
    use crate::mapping::model::InputSource;

    fn discovered() -> DiscoveredController {
        DiscoveredController {
            identity: ControllerIdentity {
                vendor_id: "0079".into(),
                product_id: "0006".into(),
                serial: Some("S9".into()),
                physical_path: Some("pci-usb-0:2".into()),
                name: "Pad".into(),
            },
            device_path: "/dev/input/event3".into(),
            devpath: "/devices/pci/input/input3".into(),
        }
    }

    fn state_in_tmp() -> AppState {
        let tmp = std::env::temp_dir().join(format!(
            "xxmapper-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        AppState {
            config_dir: tmp,
            config: AppConfig::default(),
        }
    }

    #[test]
    fn update_controller_creates_and_mutates_entry() {
        let dc = discovered();
        let mut state = state_in_tmp();
        assert!(state.controller_for(&dc).is_none());

        state.update_controller(&dc, |entry| {
            entry.enabled = true;
            entry.mapping.a = Some(InputSource::key(304));
        });

        let entry = state.controller_for(&dc).unwrap();
        assert!(entry.enabled);
        assert_eq!(entry.mapping.a, Some(InputSource::key(304)));
    }

    #[test]
    fn set_mapping_replaces_existing_mapping() {
        let dc = discovered();
        let mut state = state_in_tmp();
        state.update_controller(&dc, |entry| {
            entry.mapping.a = Some(InputSource::key(304));
        });

        let mut replacement = ControllerMapping::default();
        replacement.b = Some(InputSource::key(305));
        state.set_mapping(&dc, replacement);

        let entry = state.controller_for(&dc).unwrap();
        assert_eq!(entry.mapping.a, None);
        assert_eq!(entry.mapping.b, Some(InputSource::key(305)));
    }
}
