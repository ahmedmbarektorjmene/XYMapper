//! Versioned JSON configuration schema.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::controllers::identity::ControllerIdentity;
use crate::mapping::model::{ControllerMapping, Layout};

/// Current configuration format version.
pub const CONFIG_VERSION: u32 = 1;

/// Root configuration document stored at
/// `~/.local/share/XXMapper/config.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    /// Format version. Missing in legacy documents, treated as `0` so
    /// `migrate()` normalizes it.
    #[serde(default)]
    pub version: u32,
    /// Keyed by the stable controller identity key (`ControllerIdentity::id`).
    #[serde(default)]
    pub controllers: BTreeMap<String, ControllerConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            controllers: BTreeMap::new(),
        }
    }
}

/// Settings for one physical controller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControllerConfig {
    pub identity: ControllerIdentity,
    /// Whether the Xbox mapping is active when the controller is present.
    #[serde(default)]
    pub enabled: bool,
    /// Name of the virtual Xbox controller. Empty means a default name.
    #[serde(default)]
    pub virtual_name: String,
    /// Predefined layout selected for this controller.
    #[serde(default)]
    pub layout: Layout,
    /// The typed mapping.
    #[serde(default)]
    pub mapping: ControllerMapping,
}

impl AppConfig {
    /// Migrate an older configuration to the current version.
    ///
    /// Missing fields are already handled by `#[serde(default)]`; this is the
    /// hook for future structural migrations. After this returns, `version`
    /// equals `CONFIG_VERSION`.
    pub fn migrate(&mut self) {
        if self.version >= CONFIG_VERSION {
            self.version = CONFIG_VERSION;
            return;
        }

        match self.version {
            0 => {
                // Version 0 never existed in releases; any document without a
                // version or with version 0 is treated as a fresh config with
                // defaulted fields.
            }
            _ => {}
        }

        self.version = CONFIG_VERSION;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::model::InputSource;

    fn sample_identity() -> ControllerIdentity {
        ControllerIdentity {
            vendor_id: "0079".into(),
            product_id: "0006".into(),
            serial: Some("SN123".into()),
            physical_path: Some("pci-0000:00:14.0-usb-0:3".into()),
            name: "Generic USB Joystick".into(),
        }
    }

    #[test]
    fn config_serializes_with_version() {
        let config = AppConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        assert!(json.contains("\"version\": 1"));
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, CONFIG_VERSION);
    }

    #[test]
    fn config_round_trip_with_controller() {
        let mut config = AppConfig::default();
        let identity = sample_identity();
        config.controllers.insert(
            identity.id(),
            ControllerConfig {
                identity: identity.clone(),
                enabled: true,
                virtual_name: "Living Room Xbox".into(),
                layout: Layout::Ps3,
                mapping: {
                    let mut mapping = crate::mapping::model::ControllerMapping::default();
                    mapping.a = Some(InputSource::key(304));
                    mapping
                },
            },
        );

        let json = serde_json::to_string_pretty(&config).unwrap();
        assert!(json.contains("Living Room Xbox"));
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, 1);
        let entry = back.controllers.get(&identity.id()).unwrap();
        assert!(entry.enabled);
        assert_eq!(entry.layout, Layout::Ps3);
        assert_eq!(entry.mapping.a, Some(InputSource::key(304)));
    }

    #[test]
    fn config_without_version_migrates_to_current() {
        let json = r#"{"controllers":{}}"#;
        let mut config: AppConfig = serde_json::from_str(json).unwrap();
        config.migrate();
        assert_eq!(config.version, CONFIG_VERSION);
    }

    #[test]
    fn missing_controller_fields_default() {
        let json = r#"{
            "version": 1,
            "controllers": {
                "0079-0006-pci-x": {
                    "identity": {
                        "vendor_id": "0079",
                        "product_id": "0006",
                        "serial": null,
                        "physical_path": "pci-x",
                        "name": "Pad"
                    }
                }
            }
        }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        let entry = config
            .controllers
            .get("0079-0006-pci-x")
            .expect("controller key must be present");
        assert!(!entry.enabled, "enabled must default to false");
        assert_eq!(entry.virtual_name, "");
        assert_eq!(entry.layout, Layout::Custom);
        assert!(entry.mapping.guide.is_none());
    }

    #[test]
    fn migrate_leaves_current_version_untouched() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let mut back: AppConfig = serde_json::from_str(&json).unwrap();
        back.migrate();
        assert_eq!(back.version, CONFIG_VERSION);
    }
}
