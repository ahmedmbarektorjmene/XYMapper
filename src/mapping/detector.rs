//! Mapping capture state machine (press/release detection).
//!
//! The detector walks the 21 controls in a fixed order. For each control it
//! waits for the physical press described by `ControlId::instruction`, then
//! requires the press to be released before the capture is committed and the
//! next control is offered. Skipping is a separate command (`skip_current`)
//! and never captures.
//!
//! Axis handling is explicit: stick axes capture a peak value while held and
//! record `invert = peak > 0` so a reversed physical axis produces the correct
//! Xbox direction at runtime. Triggers and d-pads are stored as plain
//! `InputSource`s and never inverted.

use std::collections::HashMap;

use crate::controllers::evdev::{AxisSpec, InputEvent, EV_ABS, EV_KEY};
use crate::mapping::model::{AxisMapping, ControlId, ControlKind, ControllerMapping, InputSource};

/// What the detector currently holds pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Capture {
    Key { code: u16 },
    Axis { code: u16, peak: i32, trigger: bool },
    Hat { code: u16, value: i32 },
}

/// The events the detector reports back to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectorUpdate {
    /// A physical press was detected on `control`; a release is now required.
    Pressed { control: ControlId },
    /// `control` was successfully captured after its press was released.
    Captured { control: ControlId },
    /// The last control of the session was captured.
    Finished,
}

/// Press/release state machine for one manual mapping session.
#[derive(Debug)]
pub struct MappingDetector {
    mapping: ControllerMapping,
    index: usize,
    pressed: Option<Capture>,
    axis_specs: HashMap<u16, AxisSpec>,
}

impl MappingDetector {
    /// Start a session, continuing from an existing mapping.
    pub fn new(mapping: ControllerMapping) -> Self {
        Self {
            mapping,
            index: 0,
            pressed: None,
            axis_specs: HashMap::new(),
        }
    }

    /// Provide the axis metadata used to decide when an axis is pressed or
    /// released. Called once with the capabilities of the physical device.
    pub fn set_axis_specs(&mut self, specs: HashMap<u16, AxisSpec>) {
        self.axis_specs = specs;
    }

    /// The control currently being captured, if the session is still running.
    pub fn current(&self) -> Option<ControlId> {
        ControlId::ALL.get(self.index).copied()
    }

    /// True once every control has been captured (or skipped) and the session
    /// is complete.
    pub fn is_finished(&self) -> bool {
        self.index >= ControlId::ALL.len()
    }

    pub fn mapping(&self) -> &ControllerMapping {
        &self.mapping
    }

    pub fn mapping_owned(&self) -> ControllerMapping {
        self.mapping.clone()
    }

    /// The current 0-based capture index.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Feed raw events. Returns the updates produced by this batch.
    pub fn feed(&mut self, events: &[InputEvent]) -> Vec<DetectorUpdate> {
        let mut updates = Vec::new();

        for event in events {
            if self.is_finished() {
                break;
            }
            let Some(control) = self.current() else {
                break;
            };

            if self.pressed.is_none() {
                if let Some(capture) = self.detect_press(control, event) {
                    self.pressed = Some(capture);
                    updates.push(DetectorUpdate::Pressed { control });
                }
                continue;
            }

            // A press is held: wait for the release before advancing.
            let is_released = {
                let pressed = self.pressed.as_ref().unwrap();
                self.is_release(pressed, event)
            };

            if is_released {
                let capture = self.pressed.take().unwrap();
                self.commit(control, capture);
                updates.push(DetectorUpdate::Captured { control });
                self.index += 1;
                if self.is_finished() {
                    updates.push(DetectorUpdate::Finished);
                }
            } else if let Some(Capture::Axis { code, peak, .. }) = self.pressed.as_mut() {
                // Track the furthest deflection so the sign reflects the real
                // direction the user pushed, even on a slow press.
                if event.type_ == EV_ABS && event.code == *code && event.value.abs() > peak.abs() {
                    *peak = event.value;
                }
            }
        }

        updates
    }

    /// Skip the current control (equivalent to pressing Enter/Skip). It is left
    /// unmapped and the session advances. Returns the skipped control.
    pub fn skip_current(&mut self) -> Option<ControlId> {
        let control = self.current()?;
        // Clearing both kinds is harmless: `set_axis` is a no-op for button
        // controls and `set_button` is a no-op for stick axes.
        self.mapping.set_axis(control, None);
        self.mapping.set_button(control, None);
        self.pressed = None;
        self.index += 1;
        Some(control)
    }

    /// Jump to a specific control without capturing anything.
    pub fn jump_to(&mut self, control: ControlId) {
        if let Some(position) = ControlId::ALL.iter().position(|c| *c == control) {
            self.index = position;
            self.pressed = None;
        }
    }

    /// Human-readable status for the mapping view.
    pub fn status_text(&self) -> String {
        if self.is_finished() {
            return "All controls mapped.".to_string();
        }
        let control = self.current().unwrap();
        if self.pressed.is_some() {
            format!(
                "Held: {} — now RELEASE the control to capture it.",
                control.label()
            )
        } else {
            control.instruction().to_string()
        }
    }

    /// Detect a press for `control` from a single event.
    fn detect_press(&self, control: ControlId, event: &InputEvent) -> Option<Capture> {
        match control.kind() {
            ControlKind::Button => {
                if event.type_ == EV_KEY && event.value == 1 {
                    Some(Capture::Key { code: event.code })
                } else {
                    None
                }
            }
            ControlKind::DPad => {
                if event.type_ == EV_ABS && matches!(event.code, 16 | 17) && event.value != 0 {
                    Some(Capture::Hat {
                        code: event.code,
                        value: event.value,
                    })
                } else {
                    None
                }
            }
            ControlKind::StickAxis | ControlKind::Trigger => {
                // Triggers may be momentary buttons.
                if control.kind() == ControlKind::Trigger
                    && event.type_ == EV_KEY
                    && event.value == 1
                {
                    return Some(Capture::Key { code: event.code });
                }
                if event.type_ == EV_ABS && matches!(event.code, 0..=5) {
                    let trigger = control.kind() == ControlKind::Trigger;
                    if let Some(spec) = self.axis_specs.get(&event.code) {
                        if moved_from_rest(spec, event.value, trigger) {
                            return Some(Capture::Axis {
                                code: event.code,
                                peak: event.value,
                                trigger,
                            });
                        }
                    } else if event.value != 0 {
                        return Some(Capture::Axis {
                            code: event.code,
                            peak: event.value,
                            trigger,
                        });
                    }
                }
                None
            }
        }
    }

    /// Whether `event` releases the held `pressed` capture.
    fn is_release(&self, pressed: &Capture, event: &InputEvent) -> bool {
        match pressed {
            Capture::Key { code } => {
                event.type_ == EV_KEY && event.code == *code && event.value == 0
            }
            Capture::Hat { code, .. } => {
                event.type_ == EV_ABS && event.code == *code && event.value == 0
            }
            Capture::Axis { code, trigger, .. } => {
                if event.type_ == EV_ABS && event.code == *code {
                    match self.axis_specs.get(code) {
                        Some(spec) => released_from_rest(spec, event.value, *trigger),
                        None => true,
                    }
                } else {
                    false
                }
            }
        }
    }

    /// Store a completed capture into the mapping.
    fn commit(&mut self, control: ControlId, capture: Capture) {
        match capture {
            Capture::Key { code } => {
                self.mapping
                    .set_button(control, Some(InputSource::key(code)));
            }
            Capture::Hat { code, value } => {
                self.mapping
                    .set_button(control, Some(InputSource::hat(code, value)));
            }
            Capture::Axis {
                code,
                peak,
                trigger,
            } => {
                if !trigger {
                    self.mapping.set_axis(
                        control,
                        Some(AxisMapping::new(InputSource::axis(code), peak > 0)),
                    );
                } else {
                    self.mapping
                        .set_button(control, Some(InputSource::axis(code)));
                }
            }
        }
    }
}

/// Whether an axis value counts as pressed, relative to its resting position.
///
/// Sticks rest at `center` (usually 0); analog triggers rest at `min` (0 on
/// the common 0..255 range) and only read high while actually pressed.
fn moved_from_rest(spec: &AxisSpec, value: i32, trigger: bool) -> bool {
    if trigger {
        (value - spec.min).abs() > spec.deadzone
    } else {
        spec.is_moved(value)
    }
}

fn released_from_rest(spec: &AxisSpec, value: i32, trigger: bool) -> bool {
    if trigger {
        (value - spec.min).abs() <= spec.deadzone
    } else {
        spec.is_center(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controllers::evdev::{abs_code_name, EV_ABS, EV_KEY};

    fn key(code: u16, value: i32) -> InputEvent {
        InputEvent {
            type_: EV_KEY,
            code,
            value,
        }
    }

    fn abs(code: u16, value: i32) -> InputEvent {
        InputEvent {
            type_: EV_ABS,
            code,
            value,
        }
    }

    fn press_release(press: InputEvent, release: InputEvent) -> Vec<InputEvent> {
        vec![press, release]
    }

    fn stick_specs() -> HashMap<u16, AxisSpec> {
        let mut specs = HashMap::new();
        specs.insert(0, AxisSpec::from_range(-32768, 32767, 0));
        specs.insert(1, AxisSpec::from_range(-32768, 32767, 0));
        specs.insert(3, AxisSpec::from_range(-32768, 32767, 0));
        specs.insert(4, AxisSpec::from_range(-32768, 32767, 0));
        specs.insert(2, AxisSpec::from_range(0, 255, 0));
        specs.insert(5, AxisSpec::from_range(0, 255, 0));
        specs.insert(16, AxisSpec::from_range(-1, 1, 0));
        specs.insert(17, AxisSpec::from_range(-1, 1, 0));
        specs
    }

    fn captured_button(detector: &mut MappingDetector, control: ControlId, code: u16) {
        let updates = detector.feed(&press_release(key(code, 1), key(code, 0)));
        assert!(updates.contains(&DetectorUpdate::Captured { control }));
    }

    #[test]
    fn button_capture_requires_press_then_release() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());
        detector.jump_to(ControlId::A);

        // Press without release must not advance.
        let updates = detector.feed(&[key(304, 1)]);
        assert_eq!(
            updates,
            vec![DetectorUpdate::Pressed {
                control: ControlId::A
            }]
        );
        assert_eq!(detector.current(), Some(ControlId::A));
        assert!(detector.mapping().a.is_none());

        // Auto-repeat events (value 2) must not count as presses elsewhere.
        let updates = detector.feed(&[key(305, 2)]);
        assert!(updates.is_empty());

        // Releasing the held button commits the capture.
        let updates = detector.feed(&[key(304, 0)]);
        assert_eq!(
            updates,
            vec![DetectorUpdate::Captured {
                control: ControlId::A
            }]
        );
        assert_eq!(detector.mapping().a, Some(InputSource::key(304)));
        assert_eq!(detector.current(), Some(ControlId::B));
    }

    #[test]
    fn stick_axis_negative_peak_is_not_inverted() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());

        let updates = detector.feed(&[abs(0, -1000), abs(0, -30000), abs(0, 0)]);
        assert!(updates.contains(&DetectorUpdate::Captured {
            control: ControlId::LeftStickX
        }));
        let axis = detector.mapping().left_stick_x.as_ref().unwrap();
        assert_eq!(axis.source.code(), 0);
        assert!(!axis.invert);
    }

    #[test]
    fn stick_axis_positive_peak_is_inverted() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());

        // The user was told to push LEFT but this controller reports positive
        // values for left, so it must be inverted at runtime.
        let updates = detector.feed(&[abs(0, 30000), abs(0, 0)]);
        assert!(updates.contains(&DetectorUpdate::Captured {
            control: ControlId::LeftStickX
        }));
        let axis = detector.mapping().left_stick_x.as_ref().unwrap();
        assert!(axis.invert);
    }

    #[test]
    fn stick_axis_captures_peak_sign_even_on_slow_press() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());

        // Slow press that briefly dips toward zero must still invert based on
        // the furthest deflection.
        let updates = detector.feed(&[abs(1, 20000), abs(1, 500), abs(1, 32000), abs(1, 0)]);
        assert!(updates.contains(&DetectorUpdate::Captured {
            control: ControlId::LeftStickY
        }));
        assert!(detector.mapping().left_stick_y.as_ref().unwrap().invert);
    }

    #[test]
    fn trigger_as_axis_is_stored_without_invert() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());
        detector.jump_to(ControlId::L2);

        let updates = detector.feed(&[abs(2, 255), abs(2, 0)]);
        assert!(updates.contains(&DetectorUpdate::Captured {
            control: ControlId::L2
        }));
        assert_eq!(detector.mapping().l2, Some(InputSource::axis(2)));
    }

    #[test]
    fn trigger_as_button_is_stored_as_key() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());
        detector.jump_to(ControlId::L2);

        let updates = detector.feed(&[key(312, 1), key(312, 0)]);
        assert!(updates.contains(&DetectorUpdate::Captured {
            control: ControlId::L2
        }));
        assert_eq!(detector.mapping().l2, Some(InputSource::key(312)));
    }

    #[test]
    fn trigger_release_detects_resting_at_min() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());
        detector.jump_to(ControlId::R2);

        // A trigger rests at its minimum (0), not its center (127). A release
        // event value of 0 must therefore complete the capture.
        let updates = detector.feed(&[abs(5, 255), abs(5, 0)]);
        assert!(updates.contains(&DetectorUpdate::Captured {
            control: ControlId::R2
        }));
        assert_eq!(detector.mapping().r2, Some(InputSource::axis(5)));
    }

    #[test]
    fn hat_capture_stores_value() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());
        detector.jump_to(ControlId::DPadLeft);

        let updates = detector.feed(&[abs(16, -1), abs(16, 0)]);
        assert!(updates.contains(&DetectorUpdate::Captured {
            control: ControlId::DPadLeft
        }));
        assert_eq!(detector.mapping().dpad_left, Some(InputSource::hat(16, -1)));
        assert_eq!(
            detector.mapping().dpad_left.as_ref().unwrap().display(),
            abs_code_name(16)
        );
    }

    #[test]
    fn hat_press_on_axis_is_ignored_for_button_controls() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());
        detector.jump_to(ControlId::A);

        // A stray axis event while waiting for a button must not capture.
        let updates = detector.feed(&[abs(0, -30000)]);
        assert!(updates.is_empty());
        assert_eq!(detector.current(), Some(ControlId::A));
    }

    #[test]
    fn stick_axes_ignore_hat_events() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());

        let updates = detector.feed(&[abs(16, -1), abs(16, 0)]);
        assert!(updates.is_empty());
        assert_eq!(detector.current(), Some(ControlId::LeftStickX));
    }

    #[test]
    fn skip_leaves_control_unmapped_and_advances() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());
        detector.jump_to(ControlId::A);

        captured_button(&mut detector, ControlId::A, 304);
        assert_eq!(detector.mapping().a, Some(InputSource::key(304)));

        // Skip B.
        assert_eq!(detector.skip_current(), Some(ControlId::B));
        assert_eq!(detector.mapping().b, None);
        assert_eq!(detector.current(), Some(ControlId::X));
    }

    #[test]
    fn jump_to_moves_without_capturing() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());

        detector.jump_to(ControlId::L3);
        assert_eq!(detector.current(), Some(ControlId::L3));
        assert!(detector.mapping().l3.is_none());

        captured_button(&mut detector, ControlId::L3, 317);
        assert_eq!(detector.mapping().l3, Some(InputSource::key(317)));
    }

    #[test]
    fn full_session_captures_all_21_controls_and_finishes() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());

        let mut all_events = Vec::new();
        // Sticks: X(0), Y(1), RX(3), RY(4)
        for (code, control) in [
            (0, ControlId::LeftStickX),
            (1, ControlId::LeftStickY),
            (3, ControlId::RightStickX),
            (4, ControlId::RightStickY),
        ] {
            all_events.push(abs(code, -30000));
            all_events.push(abs(code, 0));
        }
        // Triggers: L2 axis 2, R2 axis 5.
        all_events.push(abs(2, 255));
        all_events.push(abs(2, 0));
        all_events.push(abs(5, 255));
        all_events.push(abs(5, 0));
        // D-pad hats.
        for (code, value, control) in [
            (16, -1, ControlId::DPadLeft),
            (16, 1, ControlId::DPadRight),
            (17, -1, ControlId::DPadUp),
            (17, 1, ControlId::DPadDown),
        ] {
            all_events.push(abs(code, value));
            all_events.push(abs(code, 0));
        }
        // Face buttons + shoulders + nav + sticks.
        for (code, control) in [
            (304, ControlId::A),
            (305, ControlId::B),
            (307, ControlId::X),
            (308, ControlId::Y),
            (310, ControlId::L1),
            (311, ControlId::R1),
            (314, ControlId::Back),
            (315, ControlId::Start),
            (316, ControlId::Guide),
            (317, ControlId::L3),
            (318, ControlId::R3),
        ] {
            all_events.push(key(code, 1));
            all_events.push(key(code, 0));
        }

        let updates = detector.feed(&all_events);
        let captured: Vec<_> = updates
            .iter()
            .filter_map(|u| match u {
                DetectorUpdate::Captured { control } => Some(*control),
                _ => None,
            })
            .collect();
        assert_eq!(captured.len(), ControlId::ALL.len());
        assert!(updates.contains(&DetectorUpdate::Finished));
        assert!(detector.is_finished());
        assert_eq!(detector.current(), None);

        // Every control got the expected source.
        assert_eq!(
            detector
                .mapping()
                .left_stick_x
                .as_ref()
                .unwrap()
                .source
                .code(),
            0
        );
        assert_eq!(
            detector
                .mapping()
                .left_stick_y
                .as_ref()
                .unwrap()
                .source
                .code(),
            1
        );
        assert_eq!(
            detector
                .mapping()
                .right_stick_x
                .as_ref()
                .unwrap()
                .source
                .code(),
            3
        );
        assert_eq!(
            detector
                .mapping()
                .right_stick_y
                .as_ref()
                .unwrap()
                .source
                .code(),
            4
        );
        assert_eq!(detector.mapping().l2, Some(InputSource::axis(2)));
        assert_eq!(detector.mapping().r2, Some(InputSource::axis(5)));
        assert_eq!(detector.mapping().dpad_left, Some(InputSource::hat(16, -1)));
        assert_eq!(detector.mapping().dpad_right, Some(InputSource::hat(16, 1)));
        assert_eq!(detector.mapping().dpad_up, Some(InputSource::hat(17, -1)));
        assert_eq!(detector.mapping().dpad_down, Some(InputSource::hat(17, 1)));
        assert_eq!(detector.mapping().a, Some(InputSource::key(304)));
        assert_eq!(detector.mapping().b, Some(InputSource::key(305)));
        assert_eq!(detector.mapping().x, Some(InputSource::key(307)));
        assert_eq!(detector.mapping().y, Some(InputSource::key(308)));
        assert_eq!(detector.mapping().l1, Some(InputSource::key(310)));
        assert_eq!(detector.mapping().r1, Some(InputSource::key(311)));
        assert_eq!(detector.mapping().back, Some(InputSource::key(314)));
        assert_eq!(detector.mapping().start, Some(InputSource::key(315)));
        assert_eq!(detector.mapping().guide, Some(InputSource::key(316)));
        assert_eq!(detector.mapping().l3, Some(InputSource::key(317)));
        assert_eq!(detector.mapping().r3, Some(InputSource::key(318)));
    }

    #[test]
    fn status_text_reflects_phase() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());
        assert_eq!(
            detector.status_text(),
            ControlId::LeftStickX.instruction().to_string()
        );
        detector.feed(&[abs(0, -30000)]);
        assert!(detector.status_text().contains("RELEASE"));
    }

    #[test]
    fn feed_after_finished_is_a_no_op() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());
        detector.jump_to(ControlId::R3);
        captured_button(&mut detector, ControlId::R3, 318);
        assert!(detector.is_finished());
        assert!(detector.feed(&[key(304, 1), key(304, 0)]).is_empty());
    }

    #[test]
    fn skip_after_finished_returns_none() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(stick_specs());
        detector.jump_to(ControlId::R3);
        captured_button(&mut detector, ControlId::R3, 318);
        assert_eq!(detector.skip_current(), None);
    }
}
