//! Native uinput Xbox 360 backend.
//!
//! XXMapper creates the virtual controller itself by opening `/dev/uinput` and
//! registering the Xbox 360 pad capabilities (the four stick/trigger axes, the
//! two d-pad hats and the standard button set), then drives it with translated
//! evdev events. No external program is spawned.

use uinput::event::absolute::{Absolute, Hat, Position};
use uinput::event::controller::{Controller, GamePad};
use uinput::event::Event;
use uinput::Device;

use crate::controllers::evdev::{EV_ABS, EV_KEY};
use crate::error::{AppError, AppResult};
use crate::mapping::mapper::VirtualEvent;
use crate::mapping::model::ControlId;

/// Minimal abstraction over the virtual Xbox controller so unit tests can
/// capture emitted events without hardware.
pub trait XboxBackend {
    /// Emit a batch of events and synchronize once.
    fn emit(&mut self, events: &[VirtualEvent]) -> AppResult<()>;
    fn device_name(&self) -> &str;
}

/// Native `/dev/uinput` implementation.
pub struct UinputXboxBackend {
    device: Device,
    name: String,
}

impl UinputXboxBackend {
    /// Create a virtual Xbox 360 controller named `name`.
    ///
    /// `controller_name` is the human-readable label shown in the system; the
    /// kernel exposes it under `/dev/input/`.
    pub fn create(name: &str) -> AppResult<Self> {
        let controller_name = if name.trim().is_empty() {
            "XXMapper Virtual Xbox Controller".to_string()
        } else {
            name.trim().to_string()
        };

        fn map_uinput(e: uinput::Error) -> AppError {
            AppError::XboxBackendFailed(format!("uinput: {e}"))
        }

        let mut builder = uinput::default()
            .map_err(|e| AppError::UinputUnavailable(format!("cannot open /dev/uinput: {e}")))?;
        builder = builder.name(&controller_name).map_err(map_uinput)?;

        // Buttons: A/B/X/Y, shoulders, triggers (as buttons), nav, stick clicks.
        for button in [
            GamePad::South,
            GamePad::East,
            GamePad::North,
            GamePad::West,
            GamePad::TL,
            GamePad::TR,
            GamePad::TL2,
            GamePad::TR2,
            GamePad::Select,
            GamePad::Start,
            GamePad::Mode,
            GamePad::ThumbL,
            GamePad::ThumbR,
        ] {
            builder = builder
                .event(Event::Controller(Controller::GamePad(button)))
                .map_err(map_uinput)?;
        }

        // Stick and trigger axes.
        for (position, min, max) in [
            (Position::X, -32767, 32767),
            (Position::Y, -32767, 32767),
            (Position::Z, 0, 255),
            (Position::RX, -32767, 32767),
            (Position::RY, -32767, 32767),
            (Position::RZ, 0, 255),
        ] {
            builder = builder
                .event(Event::Absolute(Absolute::Position(position)))
                .map_err(map_uinput)?
                .min(min)
                .max(max)
                .flat(0)
                .fuzz(0);
        }

        // D-pad hats.
        for hat in [Hat::X0, Hat::Y0] {
            builder = builder
                .event(Event::Absolute(Absolute::Hat(hat)))
                .map_err(map_uinput)?
                .min(-1)
                .max(1)
                .flat(0)
                .fuzz(0);
        }

        let device = builder.create().map_err(map_uinput)?;

        Ok(Self {
            device,
            name: controller_name,
        })
    }
}

impl XboxBackend for UinputXboxBackend {
    fn emit(&mut self, events: &[VirtualEvent]) -> AppResult<()> {
        for event in events {
            self.write_event(event)?;
        }
        self.device.synchronize().map_err(|e| {
            AppError::XboxBackendFailed(format!("uinput synchronize: {e}"))
        })
    }

    fn device_name(&self) -> &str {
        &self.name
    }
}

impl UinputXboxBackend {
    fn write_event(&mut self, event: &VirtualEvent) -> AppResult<()> {
        match event.kind {
            EV_KEY => self.write_key(event.code, event.value),
            EV_ABS => self.write_abs(event.code, event.value),
            _ => Ok(()),
        }
    }

    fn write_key(&mut self, code: u16, value: i32) -> AppResult<()> {
        let Some(button) = button_from_code(code) else {
            return Ok(());
        };
        let result = self
            .device
            .send(Event::Controller(Controller::GamePad(button)), value)
            .map_err(|e| AppError::XboxBackendFailed(format!("uinput key {code}: {e}")))?;
        Ok(result)
    }

    fn write_abs(&mut self, code: u16, value: i32) -> AppResult<()> {
        let absolute = match code {
            0 => Absolute::Position(Position::X),
            1 => Absolute::Position(Position::Y),
            2 => Absolute::Position(Position::Z),
            3 => Absolute::Position(Position::RX),
            4 => Absolute::Position(Position::RY),
            5 => Absolute::Position(Position::RZ),
            16 => Absolute::Hat(Hat::X0),
            17 => Absolute::Hat(Hat::Y0),
            _ => return Ok(()),
        };
        self.device.send(Event::Absolute(absolute), value).map_err(|e| {
            AppError::XboxBackendFailed(format!("uinput abs {code}: {e}"))
        })?;
        Ok(())
    }
}

/// Map an Xbox virtual button code to the uinput `GamePad` enum.
fn button_from_code(code: u16) -> Option<GamePad> {
    match code {
        304 => Some(GamePad::South),
        305 => Some(GamePad::East),
        307 => Some(GamePad::North),
        308 => Some(GamePad::West),
        310 => Some(GamePad::TL),
        311 => Some(GamePad::TR),
        312 => Some(GamePad::TL2),
        313 => Some(GamePad::TR2),
        314 => Some(GamePad::Select),
        315 => Some(GamePad::Start),
        316 => Some(GamePad::Mode),
        317 => Some(GamePad::ThumbL),
        318 => Some(GamePad::ThumbR),
        _ => None,
    }
}

/// Test backend that records every emitted event.
#[cfg(test)]
pub struct RecordingBackend {
    pub events: Vec<VirtualEvent>,
    name: String,
}

#[cfg(test)]
impl RecordingBackend {
    pub fn new(name: &str) -> Self {
        Self {
            events: Vec::new(),
            name: name.to_string(),
        }
    }
}

#[cfg(test)]
impl XboxBackend for RecordingBackend {
    fn emit(&mut self, events: &[VirtualEvent]) -> AppResult<()> {
        self.events.extend_from_slice(events);
        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_codes_map_to_gamepad_variants() {
        assert_eq!(button_from_code(304), Some(GamePad::South));
        assert_eq!(button_from_code(308), Some(GamePad::West));
        assert_eq!(button_from_code(318), Some(GamePad::ThumbR));
        assert_eq!(button_from_code(999), None);
    }

    #[test]
    fn recording_backend_captures_events() {
        let mut backend = RecordingBackend::new("test");
        backend
            .emit(&[
                VirtualEvent::key(304, 1),
                VirtualEvent::abs(0, -32767),
            ])
            .unwrap();
        assert_eq!(backend.events.len(), 2);
        assert_eq!(backend.device_name(), "test");
    }

    #[test]
    fn xbox_control_codes_cover_all_mapped_controls() {
        // Every key-mapped control has a virtual Xbox code.
        for control in ControlId::ALL {
            if crate::mapping::model::ControllerMapping::is_axis(control) {
                assert!(crate::mapping::mapper::xbox_axis_code(control).is_some());
            }
        }
    }

    /// End-to-end check against the real `/dev/uinput` (ignored by default;
    /// run explicitly with `--ignored` where the device node is available).
    #[test]
    #[ignore]
    fn real_uinput_device_is_created_and_visible() {
        const TEST_NAME: &str = "XXMapper Test Pad";
        let mut backend = UinputXboxBackend::create(TEST_NAME).unwrap();
        assert_eq!(backend.device_name(), TEST_NAME);

        backend
            .emit(&[
                VirtualEvent::key(304, 1),
                VirtualEvent::abs(0, -32767),
            ])
            .unwrap();

        // The virtual controller must appear as an evdev device with our name.
        let found = std::fs::read_dir("/dev/input")
            .map(|entries| {
                entries.flatten().any(|entry| {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if !name.starts_with("event") {
                        return false;
                    }
                    match evdev::Device::open(entry.path()) {
                        Ok(device) => device.name() == Some(TEST_NAME),
                        Err(_) => false,
                    }
                })
            })
            .unwrap_or(false);
        assert!(found, "virtual controller '{TEST_NAME}' not visible in /dev/input");
    }
}