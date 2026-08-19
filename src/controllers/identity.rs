//! Stable per-controller identity generation.
//!
//! Identity hierarchy (strict):
//!
//! 1. USB serial number (`ID_SERIAL_SHORT`) when a genuine stable serial is
//!    available.
//! 2. Otherwise the stable physical USB device path from udev (`ID_PATH`).
//! 3. Otherwise the controller has NO reliable persistent identity and must
//!    not automatically match a saved configuration.
//!
//! The human-readable controller name and the transient `/dev/input/eventN`
//! node are NEVER part of the identity. The udev `DEVPATH`/`inputN` numbers
//! are NOT stable across reconnects/reboots and are never used as identity
//! either.
//!
//! The semantic identity key (`stable_key`) is kept distinct from the
//! filesystem-safe `id()`/`filename()` representation: sanitization is applied
//! only to produce file/configuration keys, never to define the identity
//! itself.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// How strongly a controller identity can be established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IdentityStrength {
    /// USB serial number available.
    Serial,
    /// Stable physical USB path (`ID_PATH`) available.
    PhysicalPath,
    /// Neither serial nor physical path: the device cannot be reliably
    /// identified across reconnects and must not auto-match saved config.
    Ephemeral,
}

/// Stable identity of one physical controller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerIdentity {
    /// udev `ID_VENDOR_ID`, lowercase hex, e.g. `"0079"`.
    pub vendor_id: String,
    /// udev `ID_MODEL_ID`, lowercase hex, e.g. `"0006"`.
    pub product_id: String,
    /// Genuine USB serial number, when one exists.
    pub serial: Option<String>,
    /// Stable physical USB path (`ID_PATH`), when one exists.
    pub physical_path: Option<String>,
    /// Human-readable name. Informational only; never part of the identity.
    pub name: String,
}

/// Replace any character that is not safe in a file name with `_`.
fn sanitize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

impl ControllerIdentity {
    /// The strength of this identity.
    pub fn strength(&self) -> IdentityStrength {
        if self.serial.as_deref().is_some_and(|s| !s.trim().is_empty()) {
            IdentityStrength::Serial
        } else if self
            .physical_path
            .as_deref()
            .is_some_and(|p| !p.trim().is_empty())
        {
            IdentityStrength::PhysicalPath
        } else {
            IdentityStrength::Ephemeral
        }
    }

    /// Whether this controller can be reliably matched to a saved
    /// configuration.
    pub fn is_persistent(&self) -> bool {
        self.strength() != IdentityStrength::Ephemeral
    }

    /// The semantic stable identity key.
    ///
    /// - serial present: `{vendor}-{product}-{serial}`
    /// - otherwise path: `{vendor}-{product}-{physical-path}`
    /// - otherwise: `None` (ephemeral, must not auto-match saved config).
    ///
    /// The key never contains `/dev/input/eventN`, the udev `DEVPATH`, or the
    /// controller name.
    pub fn stable_key(&self) -> Option<String> {
        match self.strength() {
            IdentityStrength::Serial => self
                .serial
                .as_deref()
                .map(|serial| format!("{}-{}-{}", self.vendor_id, self.product_id, serial)),
            IdentityStrength::PhysicalPath => self
                .physical_path
                .as_deref()
                .map(|path| format!("{}-{}-{}", self.vendor_id, self.product_id, path)),
            IdentityStrength::Ephemeral => None,
        }
    }

    /// Filesystem-safe identity used as the configuration key.
    ///
    /// For persistent identities this is the sanitized `stable_key`. For
    /// ephemeral identities it is a VID/PID-only key; such controllers are
    /// never auto-matched to saved configurations (see `is_persistent`).
    pub fn id(&self) -> String {
        match self.stable_key() {
            Some(key) => sanitize(&key),
            None => sanitize(&format!("{}-{}", self.vendor_id, self.product_id)),
        }
    }

    /// A filename-safe variant of `id()`, suitable for per-controller files.
    pub fn filename(&self) -> String {
        format!("{}.json", self.id())
    }
}

/// Build an identity from udev-style properties. The map is keyed by udev
/// property names (`ID_VENDOR_ID`, `ID_MODEL_ID`, `ID_SERIAL_SHORT`,
/// `ID_PATH`, `NAME`).
///
/// Pure function so it can be unit-tested without udev.
pub fn identity_from_properties(props: &HashMap<String, String>) -> ControllerIdentity {
    let vendor_id = props
        .get("ID_VENDOR_ID")
        .map(String::as_str)
        .unwrap_or("0000")
        .to_string();
    let product_id = props
        .get("ID_MODEL_ID")
        .map(String::as_str)
        .unwrap_or("0000")
        .to_string();
    let serial = props
        .get("ID_SERIAL_SHORT")
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string());
    let physical_path = props
        .get("ID_PATH")
        .map(String::as_str)
        .filter(|p| !p.trim().is_empty())
        .map(|p| p.to_string());
    let name = props
        .get("NAME")
        .map(String::as_str)
        .unwrap_or("Unknown Controller")
        .to_string();

    ControllerIdentity {
        vendor_id,
        product_id,
        serial,
        physical_path,
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(
        vid: &str,
        pid: &str,
        serial: Option<&str>,
        path: Option<&str>,
        name: &str,
    ) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("ID_VENDOR_ID".to_string(), vid.to_string());
        map.insert("ID_MODEL_ID".to_string(), pid.to_string());
        if let Some(s) = serial {
            map.insert("ID_SERIAL_SHORT".to_string(), s.to_string());
        }
        if let Some(p) = path {
            map.insert("ID_PATH".to_string(), p.to_string());
        }
        map.insert("NAME".to_string(), name.to_string());
        map
    }

    #[test]
    fn serial_number_identity_is_used_when_available() {
        let a = identity_from_properties(&props(
            "0079",
            "0006",
            Some("ABC123"),
            Some("pci-0000:00:14.0-usb-0:3"),
            "Generic USB Joystick",
        ));
        let b = identity_from_properties(&props(
            "0079",
            "0006",
            Some("ABC123"),
            Some("pci-0000:00:14.0-usb-0:4"),
            "Generic USB Joystick",
        ));
        assert_eq!(a.strength(), IdentityStrength::Serial);
        assert_eq!(a.stable_key(), b.stable_key());
        assert!(a.stable_key().unwrap().contains("ABC123"));
        assert!(!a.stable_key().unwrap().contains("event"));
    }

    #[test]
    fn physical_path_identity_when_no_serial() {
        let a = identity_from_properties(&props(
            "0079",
            "0006",
            None,
            Some("pci-0000:00:14.0-usb-0:3"),
            "Generic USB Joystick",
        ));
        assert_eq!(a.serial, None);
        assert_eq!(a.strength(), IdentityStrength::PhysicalPath);
        assert!(a.stable_key().unwrap().contains("pci-0000:00:14.0-usb-0:3"));
        assert!(!a.stable_key().unwrap().contains("event"));
    }

    #[test]
    fn identical_controllers_on_different_ports_have_different_identity() {
        let a = identity_from_properties(&props(
            "0079",
            "0006",
            None,
            Some("pci-0000:00:14.0-usb-0:3"),
            "Generic USB Joystick",
        ));
        let b = identity_from_properties(&props(
            "0079",
            "0006",
            None,
            Some("pci-0000:00:14.0-usb-0:4"),
            "Generic USB Joystick",
        ));
        assert_ne!(a.id(), b.id());
    }

    #[test]
    fn same_path_same_vidpid_is_deterministic() {
        let a = identity_from_properties(&props(
            "0079",
            "0006",
            None,
            Some("pci-0000:00:14.0-usb-0:3"),
            "Generic USB Joystick",
        ));
        let b = identity_from_properties(&props(
            "0079",
            "0006",
            None,
            Some("pci-0000:00:14.0-usb-0:3"),
            "Generic USB Joystick",
        ));
        assert_eq!(a.id(), b.id());
        assert_eq!(a.filename(), b.filename());
    }

    #[test]
    fn name_never_affects_identity() {
        let a = identity_from_properties(&props(
            "0079",
            "0006",
            None,
            Some("pci-0000:00:14.0-usb-0:3"),
            "Name A",
        ));
        let b = identity_from_properties(&props(
            "0079",
            "0006",
            None,
            Some("pci-0000:00:14.0-usb-0:3"),
            "Name B",
        ));
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn eventn_never_used_as_identity() {
        let a = identity_from_properties(&props(
            "0079",
            "0006",
            None,
            Some("pci-0000:00:14.0-usb-0:3"),
            "Generic USB Joystick",
        ));
        assert!(!a.id().contains("/dev/input"));
        assert!(!a.id().contains("event"));
    }

    #[test]
    fn no_serial_no_path_is_ephemeral() {
        let identity = identity_from_properties(&props("0079", "0006", None, None, "Mystery"));
        assert_eq!(identity.strength(), IdentityStrength::Ephemeral);
        assert!(!identity.is_persistent());
        assert_eq!(identity.stable_key(), None);
        // Ephemeral id is VID/PID only and may collide, which is why it must
        // never auto-match.
        assert_eq!(identity.id(), "0079-0006");
    }

    #[test]
    fn ephemeral_identity_must_not_automatic_match() {
        let saved = identity_from_properties(&props(
            "0079",
            "0006",
            None,
            Some("pci-0000:00:14.0-usb-0:3"),
            "A",
        ));
        let ephemeral = identity_from_properties(&props("0079", "0006", None, None, "B"));
        assert!(
            !ephemeral.is_persistent() || saved.stable_key() != ephemeral.stable_key(),
            "an ephemeral controller must never claim a persistent identity"
        );
    }

    #[test]
    fn devpath_is_ignored_by_identity() {
        let mut map = props("1234", "5678", None, None, "Thing");
        map.insert(
            "DEVPATH".to_string(),
            "/devices/pci0000:00/0000:00:14.0/usb3/3-2/input/input5/event12".to_string(),
        );
        let identity = identity_from_properties(&map);
        assert_eq!(identity.strength(), IdentityStrength::Ephemeral);
        assert_eq!(identity.stable_key(), None);
        assert!(!identity.id().contains("event12"));
    }

    #[test]
    fn filename_is_sanitized_but_semantic_key_is_not() {
        let identity = identity_from_properties(&props(
            "0079",
            "0006",
            None,
            Some("pci-0000:00:14.0-usb-0:3"),
            "Pad",
        ));
        let semantic = identity.stable_key().unwrap();
        assert!(semantic.contains(':'));
        let filename = identity.filename();
        assert!(!filename.contains(':'));
        assert!(filename.ends_with(".json"));
    }
}
