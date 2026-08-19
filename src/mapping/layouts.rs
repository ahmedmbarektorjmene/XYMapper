//! Predefined PS3 / PS4 controller layouts.
//!
//! Applying a layout replaces the whole mapping with the well-known evdev
//! codes used by the official Sony controllers on Linux, so a DualShock 3 or
//! DualShock 4 can be used with an Xbox-shaped mapping without manual capture.
//!
//! Axis direction handling:
//! - `ABS_X`-class axes report left as negative on both official pads, so no
//!   X inversion is needed.
//! - The DualShock 3 reports `ABS_Y`/`ABS_RY` with up negative (like the Xbox
//!   convention), so no Y inversion is needed.
//! - The DualShock 4 reports `ABS_Y`/`ABS_RY` with up **positive** (the well
//!   known DS4 quirk), so both Y axes are marked `invert` to negate the raw
//!   value at runtime.

use crate::mapping::model::{AxisMapping, ControllerMapping, InputSource, Layout};

/// The complete predefined mapping for a layout (`Custom` is empty).
pub fn layout_mapping(layout: Layout) -> ControllerMapping {
    match layout {
        Layout::Custom => ControllerMapping::default(),
        Layout::Ps3 => ps3_mapping(),
        Layout::Ps4 => ps4_mapping(),
    }
}

/// DualShock 3 / Sixaxis mapping.
pub fn ps3_mapping() -> ControllerMapping {
    ControllerMapping {
        left_stick_x: Some(AxisMapping::new(InputSource::axis(0), false)),
        left_stick_y: Some(AxisMapping::new(InputSource::axis(1), false)),
        right_stick_x: Some(AxisMapping::new(InputSource::axis(3), false)),
        right_stick_y: Some(AxisMapping::new(InputSource::axis(4), false)),

        l2: Some(InputSource::axis(2)),
        r2: Some(InputSource::axis(5)),

        dpad_left: Some(InputSource::hat(16, -1)),
        dpad_right: Some(InputSource::hat(16, 1)),
        dpad_up: Some(InputSource::hat(17, -1)),
        dpad_down: Some(InputSource::hat(17, 1)),

        a: Some(InputSource::key(304)),
        b: Some(InputSource::key(305)),
        x: Some(InputSource::key(308)),
        y: Some(InputSource::key(307)),

        l1: Some(InputSource::key(310)),
        r1: Some(InputSource::key(311)),

        back: Some(InputSource::key(314)),
        start: Some(InputSource::key(315)),
        guide: Some(InputSource::key(316)),

        l3: Some(InputSource::key(317)),
        r3: Some(InputSource::key(318)),
    }
}

/// DualShock 4 mapping (Y axes inverted to correct the DS4 quirk).
pub fn ps4_mapping() -> ControllerMapping {
    ControllerMapping {
        left_stick_x: Some(AxisMapping::new(InputSource::axis(0), false)),
        left_stick_y: Some(AxisMapping::new(InputSource::axis(1), true)),
        right_stick_x: Some(AxisMapping::new(InputSource::axis(3), false)),
        right_stick_y: Some(AxisMapping::new(InputSource::axis(4), true)),

        l2: Some(InputSource::axis(2)),
        r2: Some(InputSource::axis(5)),

        dpad_left: Some(InputSource::hat(16, -1)),
        dpad_right: Some(InputSource::hat(16, 1)),
        dpad_up: Some(InputSource::hat(17, -1)),
        dpad_down: Some(InputSource::hat(17, 1)),

        a: Some(InputSource::key(304)),
        b: Some(InputSource::key(305)),
        x: Some(InputSource::key(308)),
        y: Some(InputSource::key(307)),

        l1: Some(InputSource::key(310)),
        r1: Some(InputSource::key(311)),

        back: Some(InputSource::key(314)),
        start: Some(InputSource::key(315)),
        guide: Some(InputSource::key(316)),

        l3: Some(InputSource::key(317)),
        r3: Some(InputSource::key(318)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::model::ControlId;

    #[test]
    fn ps3_layout_is_complete_with_expected_sources() {
        let mapping = layout_mapping(Layout::Ps3);
        assert!(mapping.is_complete());

        assert_eq!(
            mapping.left_stick_x,
            Some(AxisMapping::new(InputSource::axis(0), false))
        );
        assert_eq!(
            mapping.left_stick_y,
            Some(AxisMapping::new(InputSource::axis(1), false))
        );
        assert_eq!(
            mapping.right_stick_x,
            Some(AxisMapping::new(InputSource::axis(3), false))
        );
        assert_eq!(
            mapping.right_stick_y,
            Some(AxisMapping::new(InputSource::axis(4), false))
        );

        assert_eq!(mapping.l2, Some(InputSource::axis(2)));
        assert_eq!(mapping.r2, Some(InputSource::axis(5)));

        assert_eq!(mapping.dpad_left, Some(InputSource::hat(16, -1)));
        assert_eq!(mapping.dpad_right, Some(InputSource::hat(16, 1)));
        assert_eq!(mapping.dpad_up, Some(InputSource::hat(17, -1)));
        assert_eq!(mapping.dpad_down, Some(InputSource::hat(17, 1)));

        assert_eq!(mapping.a, Some(InputSource::key(304)));
        assert_eq!(mapping.b, Some(InputSource::key(305)));
        assert_eq!(mapping.x, Some(InputSource::key(308)));
        assert_eq!(mapping.y, Some(InputSource::key(307)));
        assert_eq!(mapping.l1, Some(InputSource::key(310)));
        assert_eq!(mapping.r1, Some(InputSource::key(311)));
        assert_eq!(mapping.back, Some(InputSource::key(314)));
        assert_eq!(mapping.start, Some(InputSource::key(315)));
        assert_eq!(mapping.guide, Some(InputSource::key(316)));
        assert_eq!(mapping.l3, Some(InputSource::key(317)));
        assert_eq!(mapping.r3, Some(InputSource::key(318)));
    }

    #[test]
    fn ps4_layout_inverts_y_but_not_x() {
        let mapping = layout_mapping(Layout::Ps4);
        assert!(mapping.is_complete());

        assert_eq!(mapping.left_stick_x.as_ref().unwrap().invert, false);
        assert_eq!(mapping.left_stick_y.as_ref().unwrap().invert, true);
        assert_eq!(mapping.right_stick_x.as_ref().unwrap().invert, false);
        assert_eq!(mapping.right_stick_y.as_ref().unwrap().invert, true);
    }

    #[test]
    fn ps4_layout_keeps_sony_face_buttons() {
        let mapping = layout_mapping(Layout::Ps4);
        assert_eq!(mapping.a, Some(InputSource::key(304))); // Cross
        assert_eq!(mapping.b, Some(InputSource::key(305))); // Circle
        assert_eq!(mapping.x, Some(InputSource::key(308))); // Square
        assert_eq!(mapping.y, Some(InputSource::key(307))); // Triangle
    }

    #[test]
    fn custom_layout_is_empty() {
        let mapping = layout_mapping(Layout::Custom);
        assert!(!mapping.is_complete());
        assert!(mapping.a.is_none());
        assert!(mapping.guide.is_none());
    }

    #[test]
    fn every_control_is_assigned_in_predefined_layouts() {
        for layout in [Layout::Ps3, Layout::Ps4] {
            let mapping = layout_mapping(layout);
            for control in ControlId::ALL {
                let present = if ControllerMapping::is_axis(control) {
                    mapping.axis_mapping(control).is_some()
                } else {
                    mapping.button_source(control).is_some()
                };
                assert!(present, "{layout:?} is missing {control:?}");
            }
        }
    }

    #[test]
    fn layouts_round_trip_through_serde() {
        for layout in [Layout::Ps3, Layout::Ps4] {
            let mapping = layout_mapping(layout);
            let json = serde_json::to_string(&mapping).unwrap();
            let back: ControllerMapping = serde_json::from_str(&json).unwrap();
            assert_eq!(back, mapping);
        }
    }
}
