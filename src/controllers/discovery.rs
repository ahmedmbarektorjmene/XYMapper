//! Physical controller discovery through `/dev/input` and udev.
//!
//! The controller list is produced by scanning `/dev/input/event*`, probing
//! udev properties for identity information, and verifying that the device
//! looks like a game controller from its evdev capabilities. The transient
//! `/dev/input/eventN` path is kept only as a runtime handle, never as an
//! identity.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::controllers::evdev::GAMEPAD_AXES;
use crate::controllers::identity::{identity_from_properties, ControllerIdentity};
use crate::error::{AppError, AppResult};

/// A controller found on the system right now.
#[derive(Debug, Clone)]
pub struct DiscoveredController {
    pub identity: ControllerIdentity,
    /// Runtime device node, e.g. `/dev/input/event7`. NOT a persistent identity.
    pub device_path: PathBuf,
    /// udev device path (DEVPATH), e.g. `/devices/.../input/input5`.
    pub devpath: String,
}

/// Enumerate all currently connected game controllers.
pub fn scan_controllers() -> AppResult<Vec<DiscoveredController>> {
    let mut controllers = Vec::new();

    let input_dir = Path::new("/dev/input");
    let entries = match std::fs::read_dir(input_dir) {
        Ok(entries) => entries,
        Err(e) => return Err(AppError::Message(format!("/dev/input: {e}"))),
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("event") {
            continue;
        }
        let path = input_dir.join(name.as_ref());
        match probe_event_device(&path) {
            Ok(Some(controller)) => controllers.push(controller),
            Ok(None) => {}
            Err(e) => {
                // A single unreadable device must not abort the whole scan.
                eprintln!("XXMapper: skipping {}: {e}", path.display());
            }
        }
    }

    controllers.sort_by(|a, b| a.device_path.cmp(&b.device_path));
    Ok(controllers)
}

/// Probe a single `/dev/input/eventN` device.
///
/// Returns `Ok(None)` when the device is not a game controller.
pub fn probe_event_device(path: &Path) -> AppResult<Option<DiscoveredController>> {
    let syspath = Path::new("/sys/class/input").join(path.file_name().unwrap_or_default());
    let props = udev_properties(&syspath);

    let caps = evdev_caps(path)?;
    if !caps_are_gamepad(&caps) {
        return Ok(None);
    }

    let mut props = props;
    if !props.contains_key("NAME") {
        props.insert("NAME".to_string(), caps.name.clone());
    }

    let devpath = props
        .get("DEVPATH")
        .cloned()
        .unwrap_or_else(|| syspath.display().to_string());

    let identity = identity_from_properties(&props);

    Ok(Some(DiscoveredController {
        identity,
        device_path: path.to_path_buf(),
        devpath,
    }))
}

/// Collect udev properties for a sysfs path.
fn udev_properties(syspath: &Path) -> HashMap<String, String> {
    let mut props = HashMap::new();
    if let Ok(device) = udev::Device::from_syspath(syspath) {
        for property in device.properties() {
            props.insert(
                property.name().to_string_lossy().into_owned(),
                property.value().to_string_lossy().into_owned(),
            );
        }
        props.insert(
            "DEVPATH".to_string(),
            device.devpath().to_string_lossy().into_owned(),
        );
    }
    props
}

/// evdev capability snapshot used for classification.
#[derive(Debug, Clone, Default)]
pub struct DeviceCaps {
    pub name: String,
    pub abs_axes: Vec<u16>,
    pub keys: Vec<u16>,
}

/// Read capabilities from an evdev device (used for classification).
pub fn evdev_caps(path: &Path) -> AppResult<DeviceCaps> {
    let device = evdev::Device::open(path).map_err(|e| AppError::EvdevOpenFailed {
        path: path.display().to_string(),
        source: e,
    })?;

    let mut caps = DeviceCaps {
        name: device
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| "Unknown Controller".to_string()),
        ..Default::default()
    };

    if let Some(abs) = device.supported_absolute_axes() {
        for code in GAMEPAD_AXES {
            if abs.contains(evdev::AbsoluteAxisType(code)) {
                caps.abs_axes.push(code);
            }
        }
    }

    if let Some(keys) = device.supported_keys() {
        for code in GAMEPAD_BUTTON_RANGE {
            if keys.contains(evdev::Key::new(code)) {
                caps.keys.push(code);
            }
        }
    }

    Ok(caps)
}

/// Buttons considered evidence of a game controller: the joystick range
/// `BTN_TRIGGER..=BTN_BASE6` (288–299), modern gamepad buttons through
/// `BTN_THUMBR` (300–318), and the `BTN_DPAD_*` buttons (544–547).
const GAMEPAD_BUTTON_RANGE: std::ops::RangeInclusive<u16> = 288..=318;
const GAMEPAD_DPAD_KEY_RANGE: std::ops::RangeInclusive<u16> = 544..=547;

fn is_gamepad_key(code: u16) -> bool {
    GAMEPAD_BUTTON_RANGE.contains(&code) || GAMEPAD_DPAD_KEY_RANGE.contains(&code)
}

/// Decide whether a capability snapshot looks like a game controller.
///
/// Pure function so classification is unit-testable without hardware.
pub fn caps_are_gamepad(caps: &DeviceCaps) -> bool {
    let has_game_axes = caps
        .abs_axes
        .iter()
        .any(|code| matches!(code, 0..=5 | 16 | 17));

    let has_game_buttons = caps.keys.iter().any(|code| is_gamepad_key(*code));

    has_game_axes && has_game_buttons
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joystick_with_axes_and_buttons_is_gamepad() {
        let caps = DeviceCaps {
            name: "Generic USB Joystick".into(),
            abs_axes: vec![0, 1, 2, 3, 4, 5],
            keys: vec![288, 289, 290, 291, 292],
        };
        assert!(caps_are_gamepad(&caps));
    }

    #[test]
    fn modern_pad_with_hat_is_gamepad() {
        let caps = DeviceCaps {
            name: "DualShock".into(),
            abs_axes: vec![0, 1, 3, 4, 16, 17],
            keys: vec![304, 305, 307, 308, 310, 311, 314, 315, 317, 318],
        };
        assert!(caps_are_gamepad(&caps));
    }

    #[test]
    fn keyboard_is_not_gamepad() {
        let caps = DeviceCaps {
            name: "Keyboard".into(),
            abs_axes: vec![],
            keys: vec![1, 2, 3, 28, 57],
        };
        assert!(!caps_are_gamepad(&caps));
    }

    #[test]
    fn mouse_is_not_gamepad() {
        let caps = DeviceCaps {
            name: "Mouse".into(),
            abs_axes: vec![],
            keys: vec![272, 273, 274],
        };
        assert!(!caps_are_gamepad(&caps));
    }

    #[test]
    fn axes_without_buttons_is_not_gamepad() {
        let caps = DeviceCaps {
            name: "Touchscreen".into(),
            abs_axes: vec![0, 1, 47, 48, 49],
            keys: vec![330],
        };
        assert!(!caps_are_gamepad(&caps));
    }
}
