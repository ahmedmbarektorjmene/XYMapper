//! Runtime translation of physical evdev events into Xbox virtual events.
//!
//! Pure and unit-testable: given a mapping, the physical device's axis
//! metadata, and one physical event, produce the Xbox virtual events to emit.

use std::collections::HashMap;

use crate::controllers::evdev::{AxisSpec, InputEvent, EV_ABS, EV_KEY};
use crate::mapping::model::{AxisMapping, ControlId, ControllerMapping, InputSource};

/// A single event to write to the virtual Xbox controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualEvent {
    /// `EV_KEY` (1) or `EV_ABS` (3).
    pub kind: u16,
    /// Linux input code on the Xbox controller (304..318 or 0..17).
    pub code: u16,
    pub value: i32,
}

impl VirtualEvent {
    pub fn key(code: u16, value: i32) -> Self {
        Self {
            kind: EV_KEY,
            code,
            value,
        }
    }

    pub fn abs(code: u16, value: i32) -> Self {
        Self {
            kind: EV_ABS,
            code,
            value,
        }
    }
}

/// Xbox virtual button codes (same values as Linux `BTN_*` constants).
const XBOX_A: u16 = 304;
const XBOX_B: u16 = 305;
const XBOX_X: u16 = 308;
const XBOX_Y: u16 = 307;
const XBOX_L1: u16 = 310;
const XBOX_R1: u16 = 311;
const XBOX_L2: u16 = 312;
const XBOX_R2: u16 = 313;
const XBOX_BACK: u16 = 314;
const XBOX_START: u16 = 315;
const XBOX_GUIDE: u16 = 316;
const XBOX_L3: u16 = 317;
const XBOX_R3: u16 = 318;

/// Xbox virtual stick axes.
const XBOX_ABS_X: u16 = 0;
const XBOX_ABS_Y: u16 = 1;
const XBOX_ABS_L2: u16 = 2;
const XBOX_ABS_RX: u16 = 3;
const XBOX_ABS_RY: u16 = 4;
const XBOX_ABS_R2: u16 = 5;

/// The Xbox key code for a button `ControlId`, if it is a key-mapped control.
pub fn xbox_key_code(control: ControlId) -> Option<u16> {
    match control {
        ControlId::A => Some(XBOX_A),
        ControlId::B => Some(XBOX_B),
        ControlId::X => Some(XBOX_X),
        ControlId::Y => Some(XBOX_Y),
        ControlId::L1 => Some(XBOX_L1),
        ControlId::R1 => Some(XBOX_R1),
        ControlId::L2 => Some(XBOX_L2),
        ControlId::R2 => Some(XBOX_R2),
        ControlId::Back => Some(XBOX_BACK),
        ControlId::Start => Some(XBOX_START),
        ControlId::Guide => Some(XBOX_GUIDE),
        ControlId::L3 => Some(XBOX_L3),
        ControlId::R3 => Some(XBOX_R3),
        _ => None,
    }
}

/// The Xbox stick axis code for a stick `ControlId`.
pub fn xbox_axis_code(control: ControlId) -> Option<u16> {
    match control {
        ControlId::LeftStickX => Some(XBOX_ABS_X),
        ControlId::LeftStickY => Some(XBOX_ABS_Y),
        ControlId::RightStickX => Some(XBOX_ABS_RX),
        ControlId::RightStickY => Some(XBOX_ABS_RY),
        _ => None,
    }
}

/// The Xbox trigger axis code for L2/R2.
pub fn xbox_trigger_code(control: ControlId) -> Option<u16> {
    match control {
        ControlId::L2 => Some(XBOX_ABS_L2),
        ControlId::R2 => Some(XBOX_ABS_R2),
        _ => None,
    }
}

/// Translate one physical event into the Xbox virtual events to emit.
///
/// A physical button or hat usually produces at most one virtual event; a
/// physical axis can drive several controls if the mapping reuses it (for
/// example a button on one pad used for two Xbox controls).
pub fn translate_event(
    mapping: &ControllerMapping,
    specs: &HashMap<u16, AxisSpec>,
    event: &InputEvent,
) -> Vec<VirtualEvent> {
    match event.type_ {
        EV_KEY => translate_key(mapping, event),
        EV_ABS => translate_abs(mapping, specs, event),
        _ => Vec::new(),
    }
}

fn translate_key(mapping: &ControllerMapping, event: &InputEvent) -> Vec<VirtualEvent> {
    if event.value != 0 && event.value != 1 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for control in ControlId::ALL {
        let source = match mapping.button_source(control) {
            Some(source) => source,
            None => continue,
        };
        if let InputSource::Key { code, .. } = source {
            if code == event.code {
                if let Some(xbox_code) = xbox_key_code(control) {
                    out.push(VirtualEvent::key(xbox_code, event.value));
                }
            }
        }
    }
    out
}

fn translate_abs(
    mapping: &ControllerMapping,
    specs: &HashMap<u16, AxisSpec>,
    event: &InputEvent,
) -> Vec<VirtualEvent> {
    let mut out = Vec::new();

    // Sticks.
    for control in [ControlId::LeftStickX, ControlId::LeftStickY, ControlId::RightStickX, ControlId::RightStickY] {
        let Some(axis) = mapping.axis_mapping(control) else {
            continue;
        };
        if axis.source.code() != event.code {
            continue;
        }
        let value = scale_stick(specs.get(&event.code), event.value, axis.invert);
        if let Some(code) = xbox_axis_code(control) {
            out.push(VirtualEvent::abs(code, value));
        }
    }

    // Triggers.
    for control in [ControlId::L2, ControlId::R2] {
        let Some(source) = mapping.button_source(control) else {
            continue;
        };
        let InputSource::Axis { code, .. } = source else {
            continue;
        };
        if code != event.code {
            continue;
        }
        let value = scale_trigger(specs.get(&event.code), event.value);
        if let Some(code) = xbox_trigger_code(control) {
            out.push(VirtualEvent::abs(code, value));
        }
    }

    // D-pad hats forward to the matching Xbox hat axis.
    if matches!(event.code, 16 | 17) && hat_is_mapped(mapping, event.code) {
        out.push(VirtualEvent::abs(event.code, event.value));
    }

    out
}

/// Whether any mapped control uses a hat on `hat_code`.
fn hat_is_mapped(mapping: &ControllerMapping, hat_code: u16) -> bool {
    [
        ControlId::DPadLeft,
        ControlId::DPadRight,
        ControlId::DPadUp,
        ControlId::DPadDown,
    ]
    .iter()
    .filter_map(|control| mapping.button_source(*control))
    .any(|source| matches!(source, InputSource::Hat { code, .. } if code == hat_code))
}

/// Scale a stick value to the Xbox `[-32767, 32767]` range.
///
/// Values within the dead zone become `0`. `invert` negates the result so a
/// reversed physical axis still pushes the Xbox stick in the mapped direction.
fn scale_stick(spec: Option<&AxisSpec>, value: i32, invert: bool) -> i32 {
    let scaled = match spec {
        None => value.clamp(-32767, 32767),
        Some(spec) => {
            if spec.is_center(value) {
                return 0;
            }
            let range = (spec.max - spec.min).max(1) as f64;
            let normalized = ((value - spec.min) as f64 / range) * 2.0 - 1.0;
            let scaled = (normalized * 32767.0).round() as i32;
            scaled.clamp(-32767, 32767)
        }
    };
    if invert {
        -scaled
    } else {
        scaled
    }
}

/// Scale a trigger value to the Xbox `[0, 255]` range.
fn scale_trigger(spec: Option<&AxisSpec>, value: i32) -> i32 {
    match spec {
        None => value.clamp(0, 255),
        Some(spec) => {
            if value <= spec.min {
                return 0;
            }
            let range = (spec.max - spec.min).max(1) as f64;
            let scaled = ((value - spec.min) as f64 / range * 255.0).round() as i32;
            scaled.clamp(0, 255)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::model::AxisMapping;

    fn signed_specs() -> HashMap<u16, AxisSpec> {
        let mut specs = HashMap::new();
        specs.insert(0, AxisSpec::from_range(-32768, 32767, 0));
        specs.insert(1, AxisSpec::from_range(-32768, 32767, 0));
        specs.insert(2, AxisSpec::from_range(0, 255, 0));
        specs.insert(3, AxisSpec::from_range(-32768, 32767, 0));
        specs.insert(4, AxisSpec::from_range(-32768, 32767, 0));
        specs.insert(5, AxisSpec::from_range(0, 255, 0));
        specs
    }

    fn ps3_mapping() -> ControllerMapping {
        crate::mapping::layouts::ps3_mapping()
    }

    #[test]
    fn button_press_maps_to_xbox() {
        let events = translate_event(
            &ps3_mapping(),
            &signed_specs(),
            &InputEvent {
                type_: EV_KEY,
                code: 304,
                value: 1,
            },
        );
        assert_eq!(events, vec![VirtualEvent::key(XBOX_A, 1)]);
    }

    #[test]
    fn button_release_maps_to_xbox() {
        let events = translate_event(
            &ps3_mapping(),
            &signed_specs(),
            &InputEvent {
                type_: EV_KEY,
                code: 305,
                value: 0,
            },
        );
        assert_eq!(events, vec![VirtualEvent::key(XBOX_B, 0)]);
    }

    #[test]
    fn unmapped_button_produces_nothing() {
        let mut mapping = ControllerMapping::default();
        mapping.a = Some(InputSource::key(304));
        let events = translate_event(
            &mapping,
            &signed_specs(),
            &InputEvent {
                type_: EV_KEY,
                code: 311,
                value: 1,
            },
        );
        assert!(events.is_empty());
    }

    #[test]
    fn stick_left_scales_to_negative() {
        let events = translate_event(
            &ps3_mapping(),
            &signed_specs(),
            &InputEvent {
                type_: EV_ABS,
                code: 0,
                value: -32768,
            },
        );
        let event = events
            .iter()
            .find(|e| e.kind == EV_ABS && e.code == XBOX_ABS_X)
            .unwrap();
        assert_eq!(event.value, -32767);
    }

    #[test]
    fn stick_center_produces_zero() {
        let events = translate_event(
            &ps3_mapping(),
            &signed_specs(),
            &InputEvent {
                type_: EV_ABS,
                code: 0,
                value: 1200,
            },
        );
        let event = events
            .iter()
            .find(|e| e.kind == EV_ABS && e.code == XBOX_ABS_X)
            .unwrap();
        assert_eq!(event.value, 0);
    }

    #[test]
    fn inverted_stick_negates_value() {
        let mut mapping = ps3_mapping();
        mapping.left_stick_y = Some(AxisMapping::new(InputSource::axis(1), true));
        let events = translate_event(
            &mapping,
            &signed_specs(),
            &InputEvent {
                type_: EV_ABS,
                code: 1,
                value: 32767,
            },
        );
        let event = events
            .iter()
            .find(|e| e.kind == EV_ABS && e.code == XBOX_ABS_Y)
            .unwrap();
        // Up on the physical pad is positive, but the mapping is inverted, so
        // the Xbox stick must report up as negative.
        assert_eq!(event.value, -32767);
    }

    #[test]
    fn trigger_scales_to_255_range() {
        let events = translate_event(
            &ps3_mapping(),
            &signed_specs(),
            &InputEvent {
                type_: EV_ABS,
                code: 2,
                value: 255,
            },
        );
        let event = events
            .iter()
            .find(|e| e.kind == EV_ABS && e.code == XBOX_ABS_L2)
            .unwrap();
        assert_eq!(event.value, 255);
    }

    #[test]
    fn released_trigger_is_zero() {
        let events = translate_event(
            &ps3_mapping(),
            &signed_specs(),
            &InputEvent {
                type_: EV_ABS,
                code: 5,
                value: 0,
            },
        );
        let event = events
            .iter()
            .find(|e| e.kind == EV_ABS && e.code == XBOX_ABS_R2)
            .unwrap();
        assert_eq!(event.value, 0);
    }

    #[test]
    fn hat_forwards_to_xbox_hat() {
        let events = translate_event(
            &ps3_mapping(),
            &signed_specs(),
            &InputEvent {
                type_: EV_ABS,
                code: 16,
                value: -1,
            },
        );
        assert_eq!(events, vec![VirtualEvent::abs(16, -1)]);
    }

    #[test]
    fn unhatted_axis_is_not_forwarded_as_hat() {
        let mut mapping = ControllerMapping::default();
        mapping.dpad_left = Some(InputSource::key(304));
        let events = translate_event(
            &mapping,
            &signed_specs(),
            &InputEvent {
                type_: EV_ABS,
                code: 16,
                value: -1,
            },
        );
        assert!(events.is_empty());
    }

    #[test]
    fn shared_physical_axis_drives_two_xbox_controls() {
        // Map both L2 and R2 to physical axis 2.
        let mut mapping = ControllerMapping::default();
        mapping.l2 = Some(InputSource::axis(2));
        mapping.r2 = Some(InputSource::axis(2));
        let events = translate_event(
            &mapping,
            &signed_specs(),
            &InputEvent {
                type_: EV_ABS,
                code: 2,
                value: 255,
            },
        );
        assert!(events.contains(&VirtualEvent::abs(XBOX_ABS_L2, 255)));
        assert!(events.contains(&VirtualEvent::abs(XBOX_ABS_R2, 255)));
    }

    #[test]
    fn autorepeat_key_events_are_ignored() {
        let events = translate_event(
            &ps3_mapping(),
            &signed_specs(),
            &InputEvent {
                type_: EV_KEY,
                code: 304,
                value: 2,
            },
        );
        assert!(events.is_empty());
    }

    #[test]
    fn xbox_codes_match_linux_btns() {
        assert_eq!(xbox_key_code(ControlId::A), Some(304));
        assert_eq!(xbox_key_code(ControlId::B), Some(305));
        assert_eq!(xbox_key_code(ControlId::Y), Some(307));
        assert_eq!(xbox_key_code(ControlId::X), Some(308));
        assert_eq!(xbox_key_code(ControlId::L3), Some(317));
        assert_eq!(xbox_axis_code(ControlId::LeftStickY), Some(1));
        assert_eq!(xbox_trigger_code(ControlId::R2), Some(5));
    }
}