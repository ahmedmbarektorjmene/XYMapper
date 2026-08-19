//! Live mapping session: runs the `MappingDetector` against a real evdev
//! device on a background thread, translating detector updates and commands
//! into messages the UI can consume.

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::controllers::evdev::{EvdevSource, EventSource};
use crate::error::AppResult;
use crate::mapping::detector::{DetectorUpdate, MappingDetector};
use crate::mapping::model::{ControlId, ControllerMapping};

/// Commands the UI can send to the running session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingCommand {
    /// Skip the current control (leave it unmapped) and advance.
    Skip,
    /// Jump to a specific control.
    JumpTo(ControlId),
    /// Stop the session.
    Cancel,
}

/// State changes the session reports back to the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingUpdate {
    /// A press is being held on `control`.
    Held { control: ControlId },
    /// `control` was captured or skipped; `mapping` is the new full mapping and
    /// `next` is the control now being offered (`None` when the session is over).
    Changed {
        control: ControlId,
        mapping: ControllerMapping,
        next: Option<ControlId>,
    },
    /// All controls have been processed.
    Finished,
    /// The session was cancelled.
    Cancelled,
    /// The device failed.
    Error(String),
}

/// A running manual mapping session for one controller.
pub struct MappingSession {
    command_tx: Sender<MappingCommand>,
    update_rx: Option<Receiver<MappingUpdate>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MappingSession {
    /// Open `device_path`, discard any stale events, and start capturing.
    pub fn start(device_path: &Path, existing: ControllerMapping) -> AppResult<Self> {
        let mut source = EvdevSource::open(device_path)?;
        // Discard stale presses so a button held before mapping began does not
        // immediately capture the first control.
        source.drain()?;

        let (command_tx, command_rx) = channel();
        let (update_tx, update_rx) = channel();

        let handle = thread::Builder::new()
            .name("xxmapper-mapping".into())
            .spawn(move || {
                run_mapping_loop(source, existing, command_rx, update_tx);
            })
            .map_err(|e| {
                crate::error::AppError::Message(format!("failed to start mapping session: {e}"))
            })?;

        Ok(Self {
            command_tx,
            update_rx: Some(update_rx),
            handle: Some(handle),
        })
    }

    /// Hand the update receiver over to the UI. Callable once.
    pub fn take_updates(&mut self) -> Option<Receiver<MappingUpdate>> {
        self.update_rx.take()
    }

    pub fn skip(&self) {
        let _ = self.command_tx.send(MappingCommand::Skip);
    }

    pub fn jump_to(&self, control: ControlId) {
        let _ = self.command_tx.send(MappingCommand::JumpTo(control));
    }

    pub fn cancel(&self) {
        let _ = self.command_tx.send(MappingCommand::Cancel);
    }
}

impl Drop for MappingSession {
    fn drop(&mut self) {
        let _ = self.command_tx.send(MappingCommand::Cancel);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_mapping_loop(
    mut source: EvdevSource,
    existing: ControllerMapping,
    commands: Receiver<MappingCommand>,
    updates: Sender<MappingUpdate>,
) {
    let mut detector = MappingDetector::new(existing);
    let specs = (0..=5)
        .chain([16, 17])
        .filter_map(|code| source.axis_spec(code).map(|spec| (code, spec)))
        .collect();
    detector.set_axis_specs(specs);

    loop {
        // Process pending commands first so skips feel instant.
        match commands.try_recv() {
            Ok(MappingCommand::Skip) => {
                if let Some(control) = detector.skip_current() {
                    emit_changed(&mut detector, control, &updates);
                }
            }
            Ok(MappingCommand::JumpTo(control)) => {
                detector.jump_to(control);
                // Re-show the jumped-to control in the waiting phase.
                if let Some(control) = detector.current() {
                    let _ = updates.send(MappingUpdate::Changed {
                        control,
                        mapping: detector.mapping_owned(),
                        next: Some(control),
                    });
                }
            }
            Ok(MappingCommand::Cancel) => {
                let _ = updates.send(MappingUpdate::Cancelled);
                return;
            }
            Err(_) => {}
        }

        match source.next_event(Duration::from_millis(50)) {
            Ok(Some(event)) => {
                let detector_updates = detector.feed(&[event]);
                for update in detector_updates {
                    match update {
                        DetectorUpdate::Pressed { control } => {
                            let _ = updates.send(MappingUpdate::Held { control });
                        }
                        DetectorUpdate::Captured { control } => {
                            emit_changed(&mut detector, control, &updates);
                        }
                        DetectorUpdate::Finished => {
                            let _ = updates.send(MappingUpdate::Finished);
                            return;
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                let _ = updates.send(MappingUpdate::Error(e.to_string()));
                return;
            }
        }
    }
}

fn emit_changed(
    detector: &mut MappingDetector,
    control: ControlId,
    updates: &Sender<MappingUpdate>,
) {
    let mapping = detector.mapping_owned();
    let next = detector.current();
    let _ = updates.send(MappingUpdate::Changed {
        control,
        mapping,
        next,
    });
    if next.is_none() {
        let _ = updates.send(MappingUpdate::Finished);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controllers::evdev::AxisSpec;
    use crate::mapping::detector::MappingDetector;

    // The runner is glued together from pure pieces; verify the command wiring
    // behaves by driving a detector directly through the same helper used by
    // `run_mapping_loop`.
    #[test]
    fn skip_emits_changed_with_next_control() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(
            (0..=5)
                .chain([16, 17])
                .map(|c| (c, AxisSpec::from_range(0, 255, 0)))
                .collect(),
        );
        let (updates_tx, updates_rx) = channel();

        let control = detector.current().unwrap();
        assert_eq!(control, ControlId::LeftStickX);
        detector.skip_current();
        emit_changed(&mut detector, control, &updates_tx);

        let update = updates_rx.try_recv().unwrap();
        match update {
            MappingUpdate::Changed { mapping, next, .. } => {
                assert_eq!(next, Some(ControlId::LeftStickY));
                assert!(mapping.left_stick_x.is_none());
            }
            _ => panic!("expected Changed"),
        }
        assert!(detector.current().is_some());
    }

    #[test]
    fn finished_is_emitted_after_last_control() {
        let mut detector = MappingDetector::new(ControllerMapping::default());
        detector.set_axis_specs(
            (0..=5)
                .chain([16, 17])
                .map(|c| (c, AxisSpec::from_range(0, 255, 0)))
                .collect(),
        );
        let (updates_tx, updates_rx) = channel();

        detector.jump_to(ControlId::R3);
        let control = detector.current().unwrap();
        detector.skip_current();
        emit_changed(&mut detector, control, &updates_tx);

        let first = updates_rx.try_recv().unwrap();
        match first {
            MappingUpdate::Changed { next, .. } => assert_eq!(next, None),
            _ => panic!("expected Changed"),
        }
        let second = updates_rx.try_recv().unwrap();
        assert_eq!(second, MappingUpdate::Finished);
    }
}
