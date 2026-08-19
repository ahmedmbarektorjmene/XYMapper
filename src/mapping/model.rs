//! Typed, serializable controller mapping model.
//!
//! The core mapping is never represented as fragile strings. Every Xbox
//! control is either an `AxisMapping` (stick axes, with an explicit `invert`
//! flag) or an optional `InputSource` (buttons, triggers, d-pad directions).

use serde::{Deserialize, Serialize};

use crate::controllers::evdev::{abs_code_name, key_code_name};

/// Physical input captured from evdev.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputSource {
    /// A button/switch (`EV_KEY`). `code` is the raw evdev key code.
    Key { code: u16, name: String },
    /// An absolute axis (`EV_ABS`). `code` is the raw evdev axis code.
    Axis { code: u16, name: String },
    /// A hat/d-pad direction (`EV_ABS` with a direction value).
    Hat { code: u16, value: i32, name: String },
}

impl InputSource {
    pub fn key(code: u16) -> Self {
        Self::Key {
            code,
            name: key_code_name(code),
        }
    }

    pub fn axis(code: u16) -> Self {
        Self::Axis {
            code,
            name: abs_code_name(code),
        }
    }

    pub fn hat(code: u16, value: i32) -> Self {
        Self::Hat {
            code,
            value,
            name: abs_code_name(code),
        }
    }

    /// The raw evdev code of this source.
    pub fn code(&self) -> u16 {
        match self {
            InputSource::Key { code, .. }
            | InputSource::Axis { code, .. }
            | InputSource::Hat { code, .. } => *code,
        }
    }

    /// Human-readable name (e.g. `BTN_SOUTH`, `ABS_Y`, `ABS_HAT0X`).
    pub fn display(&self) -> String {
        match self {
            InputSource::Key { name, .. }
            | InputSource::Axis { name, .. }
            | InputSource::Hat { name, .. } => name.clone(),
        }
    }
}

/// A stick-axis mapping. `invert` is explicit: `true` means the raw value is
/// negated before scaling so that the physical direction captured during
/// mapping produces the correct Xbox direction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxisMapping {
    pub source: InputSource,
    pub invert: bool,
}

impl AxisMapping {
    pub fn new(source: InputSource, invert: bool) -> Self {
        Self { source, invert }
    }
}

/// The predefined mapping layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Custom,
    Ps3,
    Ps4,
}

impl Layout {
    pub fn label(&self) -> &'static str {
        match self {
            Layout::Custom => "Custom",
            Layout::Ps3 => "PS3",
            Layout::Ps4 => "PS4",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "Custom" => Some(Layout::Custom),
            "PS3" => Some(Layout::Ps3),
            "PS4" => Some(Layout::Ps4),
            _ => None,
        }
    }
}

/// The kind of capture a control uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlKind {
    /// Analog stick axis; released when back near center.
    StickAxis,
    /// Analog trigger (axis or button).
    Trigger,
    /// Momentary button.
    Button,
    /// D-pad direction (hat value or d-pad key).
    DPad,
}

/// One of the 21 Xbox controls the user can map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlId {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    L2,
    R2,
    DPadLeft,
    DPadRight,
    DPadUp,
    DPadDown,
    A,
    B,
    X,
    Y,
    L1,
    R1,
    Back,
    Start,
    Guide,
    L3,
    R3,
}

/// The physical direction requested while capturing a stick axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StickDirection {
    Left,
    Up,
}

impl ControlId {
    pub const ALL: [ControlId; 21] = [
        ControlId::LeftStickX,
        ControlId::LeftStickY,
        ControlId::RightStickX,
        ControlId::RightStickY,
        ControlId::L2,
        ControlId::R2,
        ControlId::DPadLeft,
        ControlId::DPadRight,
        ControlId::DPadUp,
        ControlId::DPadDown,
        ControlId::A,
        ControlId::B,
        ControlId::X,
        ControlId::Y,
        ControlId::L1,
        ControlId::R1,
        ControlId::Back,
        ControlId::Start,
        ControlId::Guide,
        ControlId::L3,
        ControlId::R3,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ControlId::LeftStickX => "left_stick_x",
            ControlId::LeftStickY => "left_stick_y",
            ControlId::RightStickX => "right_stick_x",
            ControlId::RightStickY => "right_stick_y",
            ControlId::L2 => "l2",
            ControlId::R2 => "r2",
            ControlId::DPadLeft => "dpad_left",
            ControlId::DPadRight => "dpad_right",
            ControlId::DPadUp => "dpad_up",
            ControlId::DPadDown => "dpad_down",
            ControlId::A => "a",
            ControlId::B => "b",
            ControlId::X => "x",
            ControlId::Y => "y",
            ControlId::L1 => "l1",
            ControlId::R1 => "r1",
            ControlId::Back => "back",
            ControlId::Start => "start",
            ControlId::Guide => "guide",
            ControlId::L3 => "l3",
            ControlId::R3 => "r3",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        ControlId::ALL
            .iter()
            .copied()
            .find(|id| id.as_str() == value)
    }

    /// Display label using L1/L2/L3, R1/R2/R3 terminology.
    pub fn label(&self) -> &'static str {
        match self {
            ControlId::LeftStickX => "Left Stick X",
            ControlId::LeftStickY => "Left Stick Y",
            ControlId::RightStickX => "Right Stick X",
            ControlId::RightStickY => "Right Stick Y",
            ControlId::L2 => "L2",
            ControlId::R2 => "R2",
            ControlId::DPadLeft => "D-Pad Left",
            ControlId::DPadRight => "D-Pad Right",
            ControlId::DPadUp => "D-Pad Up",
            ControlId::DPadDown => "D-Pad Down",
            ControlId::A => "A",
            ControlId::B => "B",
            ControlId::X => "X",
            ControlId::Y => "Y",
            ControlId::L1 => "L1",
            ControlId::R1 => "R1",
            ControlId::Back => "Back",
            ControlId::Start => "Start",
            ControlId::Guide => "Guide",
            ControlId::L3 => "L3",
            ControlId::R3 => "R3",
        }
    }

    pub fn kind(&self) -> ControlKind {
        match self {
            ControlId::LeftStickX
            | ControlId::LeftStickY
            | ControlId::RightStickX
            | ControlId::RightStickY => ControlKind::StickAxis,
            ControlId::L2 | ControlId::R2 => ControlKind::Trigger,
            ControlId::DPadLeft
            | ControlId::DPadRight
            | ControlId::DPadUp
            | ControlId::DPadDown => ControlKind::DPad,
            _ => ControlKind::Button,
        }
    }

    /// Direction the user is told to move for a stick axis. Left for X axes,
    /// Up for Y axes.
    pub fn stick_direction(&self) -> Option<StickDirection> {
        match self {
            ControlId::LeftStickX | ControlId::RightStickX => Some(StickDirection::Left),
            ControlId::LeftStickY | ControlId::RightStickY => Some(StickDirection::Up),
            _ => None,
        }
    }

    pub fn instruction(&self) -> &'static str {
        match self {
            ControlId::LeftStickX => "Move the LEFT STICK fully LEFT, then RELEASE it.",
            ControlId::LeftStickY => "Move the LEFT STICK fully UP, then RELEASE it.",
            ControlId::RightStickX => "Move the RIGHT STICK fully LEFT, then RELEASE it.",
            ControlId::RightStickY => "Move the RIGHT STICK fully UP, then RELEASE it.",
            ControlId::L2 => "Press L2 fully, then RELEASE it.",
            ControlId::R2 => "Press R2 fully, then RELEASE it.",
            ControlId::DPadLeft => "Press D-PAD LEFT, then RELEASE it.",
            ControlId::DPadRight => "Press D-PAD RIGHT, then RELEASE it.",
            ControlId::DPadUp => "Press D-PAD UP, then RELEASE it.",
            ControlId::DPadDown => "Press D-PAD DOWN, then RELEASE it.",
            ControlId::A => "Press A, then RELEASE it.",
            ControlId::B => "Press B, then RELEASE it.",
            ControlId::X => "Press X, then RELEASE it.",
            ControlId::Y => "Press Y, then RELEASE it.",
            ControlId::L1 => "Press L1, then RELEASE it.",
            ControlId::R1 => "Press R1, then RELEASE it.",
            ControlId::Back => "Press BACK / SELECT, then RELEASE it.",
            ControlId::Start => "Press START, then RELEASE it.",
            ControlId::Guide => "Press GUIDE / HOME if your controller has one, then RELEASE it.",
            ControlId::L3 => "CLICK the LEFT STICK, then RELEASE it.",
            ControlId::R3 => "CLICK the RIGHT STICK, then RELEASE it.",
        }
    }
}

/// The complete mapping of one physical controller to an Xbox controller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ControllerMapping {
    pub left_stick_x: Option<AxisMapping>,
    pub left_stick_y: Option<AxisMapping>,
    pub right_stick_x: Option<AxisMapping>,
    pub right_stick_y: Option<AxisMapping>,

    pub l2: Option<InputSource>,
    pub r2: Option<InputSource>,

    pub dpad_left: Option<InputSource>,
    pub dpad_right: Option<InputSource>,
    pub dpad_up: Option<InputSource>,
    pub dpad_down: Option<InputSource>,

    pub a: Option<InputSource>,
    pub b: Option<InputSource>,
    pub x: Option<InputSource>,
    pub y: Option<InputSource>,

    pub l1: Option<InputSource>,
    pub r1: Option<InputSource>,

    pub back: Option<InputSource>,
    pub start: Option<InputSource>,
    pub guide: Option<InputSource>,

    pub l3: Option<InputSource>,
    pub r3: Option<InputSource>,
}

impl ControllerMapping {
    /// Whether `id` refers to a stick axis.
    pub fn is_axis(id: ControlId) -> bool {
        matches!(
            id,
            ControlId::LeftStickX
                | ControlId::LeftStickY
                | ControlId::RightStickX
                | ControlId::RightStickY
        )
    }

    pub fn axis_mapping(&self, id: ControlId) -> Option<&AxisMapping> {
        match id {
            ControlId::LeftStickX => self.left_stick_x.as_ref(),
            ControlId::LeftStickY => self.left_stick_y.as_ref(),
            ControlId::RightStickX => self.right_stick_x.as_ref(),
            ControlId::RightStickY => self.right_stick_y.as_ref(),
            _ => None,
        }
    }

    pub fn set_axis(&mut self, id: ControlId, value: Option<AxisMapping>) {
        match id {
            ControlId::LeftStickX => self.left_stick_x = value,
            ControlId::LeftStickY => self.left_stick_y = value,
            ControlId::RightStickX => self.right_stick_x = value,
            ControlId::RightStickY => self.right_stick_y = value,
            _ => {}
        }
    }

    /// The mapped `InputSource` for a non-axis control, if any.
    pub fn button_source(&self, id: ControlId) -> Option<InputSource> {
        match id {
            ControlId::L2 => self.l2.clone(),
            ControlId::R2 => self.r2.clone(),
            ControlId::DPadLeft => self.dpad_left.clone(),
            ControlId::DPadRight => self.dpad_right.clone(),
            ControlId::DPadUp => self.dpad_up.clone(),
            ControlId::DPadDown => self.dpad_down.clone(),
            ControlId::A => self.a.clone(),
            ControlId::B => self.b.clone(),
            ControlId::X => self.x.clone(),
            ControlId::Y => self.y.clone(),
            ControlId::L1 => self.l1.clone(),
            ControlId::R1 => self.r1.clone(),
            ControlId::Back => self.back.clone(),
            ControlId::Start => self.start.clone(),
            ControlId::Guide => self.guide.clone(),
            ControlId::L3 => self.l3.clone(),
            ControlId::R3 => self.r3.clone(),
            _ => None,
        }
    }

    pub fn set_button(&mut self, id: ControlId, value: Option<InputSource>) {
        match id {
            ControlId::L2 => self.l2 = value,
            ControlId::R2 => self.r2 = value,
            ControlId::DPadLeft => self.dpad_left = value,
            ControlId::DPadRight => self.dpad_right = value,
            ControlId::DPadUp => self.dpad_up = value,
            ControlId::DPadDown => self.dpad_down = value,
            ControlId::A => self.a = value,
            ControlId::B => self.b = value,
            ControlId::X => self.x = value,
            ControlId::Y => self.y = value,
            ControlId::L1 => self.l1 = value,
            ControlId::R1 => self.r1 = value,
            ControlId::Back => self.back = value,
            ControlId::Start => self.start = value,
            ControlId::Guide => self.guide = value,
            ControlId::L3 => self.l3 = value,
            ControlId::R3 => self.r3 = value,
            _ => {}
        }
    }

    /// Whether every one of the 21 controls has a source assigned.
    pub fn is_complete(&self) -> bool {
        ControlId::ALL.iter().all(|control| {
            if Self::is_axis(*control) {
                self.axis_mapping(*control).is_some()
            } else {
                self.button_source(*control).is_some()
            }
        })
    }

    /// Human-readable summary of one control's assignment.
    pub fn summary(&self, id: ControlId) -> String {
        if Self::is_axis(id) {
            match self.axis_mapping(id) {
                Some(axis) => format!(
                    "{}{}",
                    axis.source.display(),
                    if axis.invert { " (inverted)" } else { "" }
                ),
                None => "not mapped".to_string(),
            }
        } else {
            self.button_source(id)
                .map(|s| s.display())
                .unwrap_or_else(|| "not mapped".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_source_serializes_human_readable() {
        let source = InputSource::key(304);
        let json = serde_json::to_string(&source).unwrap();
        assert!(json.contains("BTN_SOUTH"));
        let back: InputSource = serde_json::from_str(&json).unwrap();
        assert_eq!(back, source);
    }

    #[test]
    fn hat_serialization_preserves_value() {
        let source = InputSource::hat(16, -1);
        let json = serde_json::to_string(&source).unwrap();
        let back: InputSource = serde_json::from_str(&json).unwrap();
        assert_eq!(back, source);
        assert_eq!(back.code(), 16);
    }

    #[test]
    fn axis_mapping_serializes_invert_explicitly() {
        let mapping = AxisMapping::new(InputSource::axis(1), true);
        let json = serde_json::to_string(&mapping).unwrap();
        assert!(json.contains("\"invert\":true"));
        let back: AxisMapping = serde_json::from_str(&json).unwrap();
        assert!(back.invert);
    }

    #[test]
    fn controller_mapping_round_trips() {
        let mut mapping = ControllerMapping::default();
        mapping.a = Some(InputSource::key(304));
        mapping.left_stick_y = Some(AxisMapping::new(InputSource::axis(1), true));
        let json = serde_json::to_string(&mapping).unwrap();
        let back: ControllerMapping = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mapping);
        assert_eq!(back.summary(ControlId::A), "BTN_SOUTH");
        assert_eq!(back.summary(ControlId::LeftStickY), "ABS_Y (inverted)");
    }

    #[test]
    fn unset_controls_round_trip_as_none() {
        let mapping = ControllerMapping::default();
        let json = serde_json::to_string(&mapping).unwrap();
        let back: ControllerMapping = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mapping);
        assert!(back.guide.is_none());
        assert_eq!(back.summary(ControlId::Guide), "not mapped");
    }

    #[test]
    fn control_ids_round_trip_through_str() {
        for id in ControlId::ALL {
            assert_eq!(ControlId::from_str(id.as_str()), Some(id));
        }
    }

    #[test]
    fn axis_direction_metadata() {
        assert_eq!(
            ControlId::LeftStickX.stick_direction(),
            Some(StickDirection::Left)
        );
        assert_eq!(
            ControlId::RightStickY.stick_direction(),
            Some(StickDirection::Up)
        );
        assert_eq!(ControlId::A.stick_direction(), None);
    }

    #[test]
    fn layout_serialization_and_labels() {
        assert_eq!(serde_json::to_string(&Layout::Ps3).unwrap(), "\"ps3\"");
        assert_eq!(Layout::from_label("PS4"), Some(Layout::Ps4));
        assert_eq!(Layout::Custom.label(), "Custom");
    }

    #[test]
    fn set_button_and_set_axis_update_model() {
        let mut mapping = ControllerMapping::default();
        mapping.set_button(ControlId::B, Some(InputSource::key(305)));
        assert_eq!(mapping.b, Some(InputSource::key(305)));
        mapping.set_button(ControlId::B, None);
        assert_eq!(mapping.b, None);
        mapping.set_axis(
            ControlId::LeftStickX,
            Some(AxisMapping::new(InputSource::axis(0), false)),
        );
        assert!(mapping.left_stick_x.is_some());
    }
}
