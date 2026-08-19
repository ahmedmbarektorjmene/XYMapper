//! Slint UI module: generated components plus helpers that translate
//! application state into Slint-friendly values.

slint::include_modules!();

pub mod wiring;

use crate::config::model::AppConfig;
use crate::controllers::discovery::DiscoveredController;
use crate::controllers::identity::IdentityStrength;
use crate::mapping::model::{ControlId, ControllerMapping};

/// Convert one discovered controller (plus its saved configuration) into the
/// `ControllerInfo` struct shown in the controller list.
pub fn build_controller_info(
    discovered: &DiscoveredController,
    config: &AppConfig,
) -> ControllerInfo {
    let identity = &discovered.identity;
    let entry = config.controllers.get(&identity.id());

    let status = match entry {
        Some(entry) if entry.enabled => "Connected • mapping active",
        Some(_) => "Connected",
        None => "Connected • not configured",
    };

    let vid_pid = format!("{}:{}", identity.vendor_id, identity.product_id);

    // Prefer the stable physical USB path; fall back to the transient device
    // node purely as a runtime hint.
    let path = identity
        .physical_path
        .clone()
        .unwrap_or_else(|| discovered.device_path.display().to_string());

    let mut details = format!("Name: {}\n", identity.name);
    details.push_str(&format!(
        "VID:PID: {}:{}\n",
        identity.vendor_id, identity.product_id
    ));
    if let Some(serial) = &identity.serial {
        details.push_str(&format!("Serial: {serial}\n"));
    }
    if let Some(physical) = &identity.physical_path {
        details.push_str(&format!("USB path: {physical}\n"));
    }
    let strength = match identity.strength() {
        IdentityStrength::Serial => "USB serial number",
        IdentityStrength::PhysicalPath => "USB port path",
        IdentityStrength::Ephemeral => "Transient (no stable identity)",
    };
    details.push_str(&format!("Identity: {strength}\n"));
    if !identity.is_persistent() {
        details.push_str(
            "This controller cannot be reliably matched after reconnecting, so a saved \
             configuration will not be auto-applied to it.\n",
        );
    }
    details.push_str(&format!(
        "Device node: {}\n",
        discovered.device_path.display()
    ));

    ControllerInfo {
        id: identity.id().into(),
        name: identity.name.clone().into(),
        vid_pid: vid_pid.into(),
        path: path.into(),
        status: status.into(),
        details: details.into(),
    }
}

/// Build the full list of `ControllerInfo` structs for the discovered set.
pub fn build_controller_infos(
    discovered: &[DiscoveredController],
    config: &AppConfig,
) -> Vec<ControllerInfo> {
    discovered
        .iter()
        .map(|d| build_controller_info(d, config))
        .collect()
}

/// Build the list of the 21 mapping controls with their current assignment.
pub fn build_mapping_controls(mapping: &ControllerMapping) -> Vec<MappingControlInfo> {
    ControlId::ALL
        .iter()
        .map(|id| MappingControlInfo {
            id: id.as_str().into(),
            label: id.label().into(),
            value: mapping.summary(*id).into(),
        })
        .collect()
}
