//! evdev input reading with a hardware-agnostic abstraction for testing.
//!
//! The `EventSource` trait is the seam between real hardware and tests: the
//! real implementation reads `/dev/input/eventN` through `evdev` + `poll(2)`,
//! while tests feed synthetic `InputEvent`s.

use std::collections::HashMap;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::path::Path;
use std::time::Duration;

use nix::errno::Errno;
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};

use crate::error::{AppError, AppResult};

pub const EV_SYN: u16 = 0;
pub const EV_KEY: u16 = 1;
pub const EV_ABS: u16 = 3;

/// Stick / trigger axes that matter to game controllers.
pub const GAMEPAD_AXES: [u16; 8] = [0, 1, 2, 3, 4, 5, 16, 17];

/// A single hardware-independent Linux input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

/// Properties of one absolute axis used for dead-zone and scaling decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisSpec {
    pub min: i32,
    pub max: i32,
    pub center: i32,
    pub deadzone: i32,
}

impl AxisSpec {
    /// Build an `AxisSpec` from raw evdev `input_absinfo`.
    ///
    /// For centered devices (`min < 0`) the center is `0`; for devices with a
    /// non-negative range the center is the middle of the range. The dead-zone
    /// comes from the hardware `flat` value and falls back to 5% of the range.
    pub fn from_range(min: i32, max: i32, flat: i32) -> Self {
        let center = if min < 0 { 0 } else { min + (max - min) / 2 };
        let deadzone = if flat > 0 {
            flat
        } else {
            ((max - min) / 20).max(1)
        };
        Self {
            min,
            max,
            center,
            deadzone,
        }
    }

    pub fn is_center(&self, value: i32) -> bool {
        (value - self.center).abs() <= self.deadzone
    }

    pub fn is_moved(&self, value: i32) -> bool {
        !self.is_center(value)
    }
}

/// Abstraction over an input event stream. Implemented by the real evdev
/// backend and by a synthetic fake used in tests.
pub trait EventSource {
    /// Wait up to `timeout` for the next event. Returns `Ok(None)` on timeout.
    fn next_event(&mut self, timeout: Duration) -> AppResult<Option<InputEvent>>;

    /// Remove all currently queued events.
    fn drain(&mut self) -> AppResult<()>;

    /// Axis metadata for `code`, when the device reports it.
    fn axis_spec(&self, code: u16) -> Option<AxisSpec>;
}

/// Real evdev-backed event source.
pub struct EvdevSource {
    device: evdev::Device,
    axis_specs: HashMap<u16, AxisSpec>,
}

impl EvdevSource {
    pub fn open(path: &Path) -> AppResult<Self> {
        let device = match evdev::Device::open(path) {
            Ok(device) => device,
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                return Err(AppError::PermissionDenied(path.display().to_string()));
            }
            Err(e) => {
                return Err(AppError::EvdevOpenFailed {
                    path: path.display().to_string(),
                    source: e,
                });
            }
        };

        let mut axis_specs = HashMap::new();
        if let Ok(abs_state) = device.get_abs_state() {
            for code in GAMEPAD_AXES {
                let info = &abs_state[code as usize];
                if info.minimum == 0 && info.maximum == 0 {
                    continue;
                }
                axis_specs.insert(
                    code,
                    AxisSpec::from_range(info.minimum, info.maximum, info.flat),
                );
            }
        }

        Ok(Self { device, axis_specs })
    }

    fn poll_readable(&self, timeout: Duration) -> AppResult<bool> {
        let fd = self.device.as_raw_fd();
        let mut fds = [PollFd::new(
            unsafe { BorrowedFd::borrow_raw(fd) },
            PollFlags::POLLIN,
        )];
        let timeout_ms = timeout.as_millis().min(u16::MAX as u128) as u16;
        match poll(&mut fds, PollTimeout::from(timeout_ms)) {
            Ok(_) => {
                let revents = fds[0].revents().unwrap_or(PollFlags::empty());
                if revents.intersects(PollFlags::POLLERR | PollFlags::POLLHUP | PollFlags::POLLNVAL)
                {
                    return Err(AppError::ControllerNotFound);
                }
                Ok(revents.intersects(PollFlags::POLLIN | PollFlags::POLLRDNORM))
            }
            Err(Errno::EAGAIN) | Err(Errno::EINTR) => Ok(false),
            Err(_) => Err(AppError::ControllerNotFound),
        }
    }
}

impl EventSource for EvdevSource {
    fn next_event(&mut self, timeout: Duration) -> AppResult<Option<InputEvent>> {
        if !self.poll_readable(timeout)? {
            return Ok(None);
        }

        let events = self.device.fetch_events().map_err(map_device_error)?;
        for event in events {
            let event = InputEvent {
                type_: event.event_type().0,
                code: event.code(),
                value: event.value(),
            };
            if event.type_ == EV_SYN {
                continue;
            }
            return Ok(Some(event));
        }
        Ok(None)
    }

    fn drain(&mut self) -> AppResult<()> {
        loop {
            if !self.poll_readable(Duration::ZERO)? {
                return Ok(());
            }
            let events = self.device.fetch_events().map_err(map_device_error)?;
            if events.into_iter().count() == 0 {
                return Ok(());
            }
        }
    }

    fn axis_spec(&self, code: u16) -> Option<AxisSpec> {
        self.axis_specs.get(&code).copied()
    }
}

fn map_device_error(e: io::Error) -> AppError {
    if e.kind() == io::ErrorKind::PermissionDenied {
        AppError::PermissionDenied("/dev/input".to_string())
    } else {
        AppError::Io(e)
    }
}

/// Look up a human-readable name for an `EV_KEY` code.
pub fn key_code_name(code: u16) -> String {
    match code {
        1 => "KEY_ESC".into(),
        2..=11 => format!("KEY_{}", code - 1),
        288 => "BTN_TRIGGER".into(),
        289 => "BTN_THUMB".into(),
        290 => "BTN_THUMB2".into(),
        291 => "BTN_TOP".into(),
        292 => "BTN_TOP2".into(),
        293 => "BTN_PINKIE".into(),
        294 => "BTN_BASE".into(),
        295 => "BTN_BASE2".into(),
        296 => "BTN_BASE3".into(),
        297 => "BTN_BASE4".into(),
        298 => "BTN_BASE5".into(),
        299 => "BTN_BASE6".into(),
        304 => "BTN_SOUTH".into(),
        305 => "BTN_EAST".into(),
        306 => "BTN_C".into(),
        307 => "BTN_NORTH".into(),
        308 => "BTN_WEST".into(),
        309 => "BTN_Z".into(),
        310 => "BTN_TL".into(),
        311 => "BTN_TR".into(),
        312 => "BTN_TL2".into(),
        313 => "BTN_TR2".into(),
        314 => "BTN_SELECT".into(),
        315 => "BTN_START".into(),
        316 => "BTN_MODE".into(),
        317 => "BTN_THUMBL".into(),
        318 => "BTN_THUMBR".into(),
        544 => "BTN_DPAD_UP".into(),
        545 => "BTN_DPAD_DOWN".into(),
        546 => "BTN_DPAD_LEFT".into(),
        547 => "BTN_DPAD_RIGHT".into(),
        _ => format!("KEY_{code}"),
    }
}

/// Look up a human-readable name for an `EV_ABS` code.
pub fn abs_code_name(code: u16) -> String {
    match code {
        0 => "ABS_X".into(),
        1 => "ABS_Y".into(),
        2 => "ABS_Z".into(),
        3 => "ABS_RX".into(),
        4 => "ABS_RY".into(),
        5 => "ABS_RZ".into(),
        6 => "ABS_THROTTLE".into(),
        16 => "ABS_HAT0X".into(),
        17 => "ABS_HAT0Y".into(),
        18 => "ABS_HAT1X".into(),
        19 => "ABS_HAT1Y".into(),
        20 => "ABS_HAT2X".into(),
        21 => "ABS_HAT2Y".into(),
        22 => "ABS_HAT3X".into(),
        23 => "ABS_HAT3Y".into(),
        24 => "ABS_HAT4X".into(),
        25 => "ABS_HAT4Y".into(),
        _ => format!("ABS_{code}"),
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    /// A scripted fake event source driven by an in-memory queue.
    #[derive(Debug, Default)]
    pub struct FakeEventSource {
        queue: std::collections::VecDeque<InputEvent>,
        axis_specs: HashMap<u16, AxisSpec>,
    }

    impl FakeEventSource {
        pub fn new(events: Vec<InputEvent>) -> Self {
            Self {
                queue: events.into_iter().collect(),
                axis_specs: HashMap::new(),
            }
        }

        pub fn push(&mut self, event: InputEvent) {
            self.queue.push_back(event);
        }

        pub fn with_axis(mut self, code: u16, spec: AxisSpec) -> Self {
            self.axis_specs.insert(code, spec);
            self
        }

        pub fn is_empty(&self) -> bool {
            self.queue.is_empty()
        }
    }

    impl EventSource for FakeEventSource {
        fn next_event(&mut self, _timeout: Duration) -> AppResult<Option<InputEvent>> {
            Ok(self.queue.pop_front())
        }

        fn drain(&mut self) -> AppResult<()> {
            self.queue.clear();
            Ok(())
        }

        fn axis_spec(&self, code: u16) -> Option<AxisSpec> {
            self.axis_specs.get(&code).copied()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controllers::evdev::testutil::FakeEventSource;

    #[test]
    fn axis_spec_center_for_signed_range() {
        let spec = AxisSpec::from_range(-32768, 32767, 0);
        assert_eq!(spec.center, 0);
        assert!(spec.is_center(0));
        assert!(spec.is_center(1500));
        assert!(!spec.is_center(4000));
    }

    #[test]
    fn axis_spec_center_for_unsigned_range() {
        let spec = AxisSpec::from_range(0, 255, 0);
        assert_eq!(spec.center, 127);
        assert!(spec.is_center(127));
        assert!(spec.is_center(120));
        assert!(!spec.is_center(20));
        assert!(!spec.is_center(250));
    }

    #[test]
    fn axis_spec_respects_hardware_flat() {
        let spec = AxisSpec::from_range(0, 255, 30);
        assert_eq!(spec.deadzone, 30);
    }

    #[test]
    fn fake_event_source_serves_events_in_order() {
        let mut fake = FakeEventSource::new(vec![
            InputEvent {
                type_: EV_KEY,
                code: 304,
                value: 1,
            },
            InputEvent {
                type_: EV_KEY,
                code: 304,
                value: 0,
            },
        ]);
        assert_eq!(
            fake.next_event(Duration::from_millis(10)).unwrap(),
            Some(InputEvent {
                type_: EV_KEY,
                code: 304,
                value: 1
            })
        );
        assert_eq!(
            fake.next_event(Duration::from_millis(10)).unwrap(),
            Some(InputEvent {
                type_: EV_KEY,
                code: 304,
                value: 0
            })
        );
        assert_eq!(fake.next_event(Duration::from_millis(10)).unwrap(), None);
    }

    #[test]
    fn fake_event_source_drain_clears_queue() {
        let mut fake = FakeEventSource::new(vec![InputEvent {
            type_: EV_KEY,
            code: 1,
            value: 1,
        }]);
        fake.drain().unwrap();
        assert!(fake.is_empty());
    }

    #[test]
    fn key_names_cover_gamepad_codes() {
        assert_eq!(key_code_name(304), "BTN_SOUTH");
        assert_eq!(key_code_name(317), "BTN_THUMBL");
        assert_eq!(key_code_name(313), "BTN_TR2");
    }

    #[test]
    fn abs_names_cover_stick_and_hat_codes() {
        assert_eq!(abs_code_name(1), "ABS_Y");
        assert_eq!(abs_code_name(16), "ABS_HAT0X");
        assert_eq!(abs_code_name(17), "ABS_HAT0Y");
    }
}
