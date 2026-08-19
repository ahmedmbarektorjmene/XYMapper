//! Wiring between the Slint window, application state, and the controller
//! monitor. All Slint property/callback updates happen on the UI thread;
//! background work runs in the monitor thread and is drained by a timer.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::app::monitor::ControllerMonitor;
use crate::app::state::AppState;
use crate::controllers::discovery::DiscoveredController;
use crate::error::AppResult;
use crate::mapping::model::{ControlId, ControllerMapping, Layout};
use crate::mapping::session::{MappingSession, MappingUpdate};
use crate::ui::{build_controller_infos, build_mapping_controls, MainWindow};

/// Show the mapping view for a specific control in the waiting phase.
fn show_control_for(win: &MainWindow, control: ControlId) {
    let index = ControlId::ALL
        .iter()
        .position(|c| *c == control)
        .unwrap_or(0);
    win.set_mapping_current_control(control.as_str().into());
    win.set_mapping_control_name(control.label().into());
    win.set_mapping_instruction(control.instruction().into());
    win.set_mapping_progress((index + 1) as i32);
    win.set_mapping_progress_total(ControlId::ALL.len() as i32);
    win.set_mapping_status(
        "Follow the instruction below. Press the physical control, then release it. \
         Press Enter or Skip to leave a control unmapped."
            .into(),
    );
}

/// Run the XXMapper GUI. Blocks until the window is closed.
pub fn run() -> AppResult<()> {
    let state = Rc::new(RefCell::new(AppState::new()?));
    let window = MainWindow::new().map_err(|e| {
        crate::error::AppError::Message(format!("failed to create the main window: {e}"))
    })?;

    let latest: Rc<RefCell<Vec<DiscoveredController>>> = Rc::new(RefCell::new(Vec::new()));
    let selected: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let session: Rc<RefCell<Option<MappingSession>>> = Rc::new(RefCell::new(None));
    let session_rx: Rc<RefCell<Option<mpsc::Receiver<MappingUpdate>>>> =
        Rc::new(RefCell::new(None));
    let latest_mapping: Rc<RefCell<ControllerMapping>> =
        Rc::new(RefCell::new(ControllerMapping::default()));

    let (tx, rx) = mpsc::channel();
    let _monitor = ControllerMonitor::spawn(tx, Duration::from_millis(1000));

    // ---- controller selection ----
    {
        let win = window.clone_strong();
        let state = state.clone();
        let latest = latest.clone();
        let selected = selected.clone();
        window.on_controller_selected(move |id| {
            let id: String = id.to_string();
            *selected.borrow_mut() = Some(id.clone());
            let state = state.borrow();
            let latest = latest.borrow();
            if let Some(discovered) = latest.iter().find(|d| d.identity.id() == id) {
                let info = crate::ui::build_controller_info(discovered, &state.config());
                win.set_selected_id(id.into());
                win.set_selected_controller(info);
                win.set_has_selection(true);
                if let Some(entry) = state.controller_for(discovered) {
                    win.set_selected_enabled(entry.enabled);
                    win.set_selected_virtual_name(entry.virtual_name.clone().into());
                    win.set_selected_layout(entry.layout.label().to_lowercase().into());
                } else {
                    win.set_selected_enabled(false);
                    win.set_selected_virtual_name(SharedString::default());
                    win.set_selected_layout("custom".into());
                }
            }
        });
    }

    // ---- settings edits ----
    {
        let state = state.clone();
        let latest = latest.clone();
        let selected = selected.clone();
        window.on_toggle_enabled(move |enabled| {
            let Some(id) = selected.borrow().clone() else {
                return;
            };
            let Some(discovered) = latest
                .borrow()
                .iter()
                .find(|d| d.identity.id() == id)
                .cloned()
            else {
                return;
            };
            state
                .borrow_mut()
                .update_controller(&discovered, |entry| entry.enabled = enabled);
            let _ = state.borrow().save();
        });
    }
    {
        let state = state.clone();
        let latest = latest.clone();
        let selected = selected.clone();
        window.on_virtual_name_changed(move |name| {
            let Some(id) = selected.borrow().clone() else {
                return;
            };
            let Some(discovered) = latest
                .borrow()
                .iter()
                .find(|d| d.identity.id() == id)
                .cloned()
            else {
                return;
            };
            let name = name.to_string();
            state
                .borrow_mut()
                .update_controller(&discovered, |entry| entry.virtual_name = name);
            let _ = state.borrow().save();
        });
    }
    {
        let state = state.clone();
        let latest = latest.clone();
        let selected = selected.clone();
        window.on_layout_changed(move |label| {
            let Some(id) = selected.borrow().clone() else {
                return;
            };
            let Some(discovered) = latest
                .borrow()
                .iter()
                .find(|d| d.identity.id() == id)
                .cloned()
            else {
                return;
            };
            let Some(layout) = Layout::from_label(&label) else {
                return;
            };
            state
                .borrow_mut()
                .update_controller(&discovered, |entry| entry.layout = layout);
            let _ = state.borrow().save();
        });
    }
    {
        let state = state.clone();
        window.on_save_settings(move || {
            let _ = state.borrow().save();
        });
    }

    // ---- mapping session ----
    {
        let win = window.clone_strong();
        let state = state.clone();
        let latest = latest.clone();
        let selected = selected.clone();
        let session = session.clone();
        let session_rx = session_rx.clone();
        let latest_mapping = latest_mapping.clone();
        window.on_configure_mapping(move || {
            if session.borrow().is_some() {
                return;
            }
            let Some(id) = selected.borrow().clone() else {
                return;
            };
            let Some(discovered) = latest
                .borrow()
                .iter()
                .find(|d| d.identity.id() == id)
                .cloned()
            else {
                return;
            };
            let mapping = state
                .borrow()
                .controller_for(&discovered)
                .map(|entry| entry.mapping.clone())
                .unwrap_or_default();
            *latest_mapping.borrow_mut() = mapping.clone();

            let started = MappingSession::start(&discovered.device_path, mapping);
            let mut running = match started {
                Ok(running) => running,
                Err(e) => {
                    win.set_mapping_status(format!("Cannot capture: {e}").into());
                    return;
                }
            };
            let Some(updates) = running.take_updates() else {
                return;
            };
            *session.borrow_mut() = Some(running);
            *session_rx.borrow_mut() = Some(updates);

            let controls = build_mapping_controls(&latest_mapping.borrow());
            win.set_mapping_active(true);
            win.set_mapping_controls(ModelRc::new(VecModel::from(controls)));
            show_control_for(&win, ControlId::LeftStickX);
        });
    }
    {
        let session = session.clone();
        window.on_skip_mapping(move || {
            if let Some(running) = session.borrow().as_ref() {
                running.skip();
            }
        });
    }
    {
        let session = session.clone();
        window.on_jump_to(move |id| {
            let Some(control) = ControlId::from_str(id.as_str()) else {
                return;
            };
            if let Some(running) = session.borrow().as_ref() {
                running.jump_to(control);
            }
        });
    }
    {
        let session = session.clone();
        window.on_cancel_mapping(move || {
            if let Some(running) = session.borrow().as_ref() {
                running.cancel();
            }
        });
    }

    // ---- periodic refresh from the monitor and the mapping session ----
    let refresh = {
        let win = window.clone_strong();
        let latest = latest.clone();
        let selected = selected.clone();
        let state = state.clone();
        let session = session.clone();
        let session_rx = session_rx.clone();
        let latest_mapping = latest_mapping.clone();
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            while let Ok(snapshot) = rx.try_recv() {
                *latest.borrow_mut() = snapshot;
            }

            let snapshot = latest.borrow().clone();
            {
                let state = state.borrow();
                let infos = build_controller_infos(&snapshot, &state.config());
                win.set_controllers(ModelRc::new(VecModel::from(infos)));
            }

            // Drain mapping session updates.
            if let Some(updates) = session_rx.borrow().as_ref() {
                let mut ended = false;
                while let Ok(update) = updates.try_recv() {
                    match update {
                        MappingUpdate::Held { control } => {
                            win.set_mapping_status(
                                format!(
                                    "Held: {} — now RELEASE the control to capture it.",
                                    control.label()
                                )
                                .into(),
                            );
                        }
                        MappingUpdate::Changed {
                            control,
                            mapping,
                            next,
                        } => {
                            *latest_mapping.borrow_mut() = mapping.clone();
                            win.set_mapping_controls(ModelRc::new(VecModel::from(
                                build_mapping_controls(&mapping),
                            )));
                            match next {
                                Some(next_control) => {
                                    show_control_for(&win, next_control);
                                }
                                None => {
                                    ended = true;
                                }
                            }
                        }
                        MappingUpdate::Finished => {
                            // Persist the captured mapping.
                            let mapping = latest_mapping.borrow().clone();
                            if let Some(id) = selected.borrow().clone() {
                                if let Some(discovered) = latest
                                    .borrow()
                                    .iter()
                                    .find(|d| d.identity.id() == id)
                                    .cloned()
                                {
                                    state.borrow_mut().set_mapping(&discovered, mapping.clone());
                                    let _ = state.borrow().save();
                                }
                            }
                            win.set_mapping_active(false);
                            ended = true;
                        }
                        MappingUpdate::Cancelled => {
                            win.set_mapping_active(false);
                            ended = true;
                        }
                        MappingUpdate::Error(message) => {
                            win.set_mapping_status(message.into());
                            win.set_mapping_active(false);
                            ended = true;
                        }
                    }
                }
                if ended {
                    *session.borrow_mut() = None;
                    *session_rx.borrow_mut() = None;
                }
            }

            let Some(id) = selected.borrow().clone() else {
                return;
            };
            if !snapshot.iter().any(|d| d.identity.id() == id) {
                *selected.borrow_mut() = None;
                win.set_selected_id(SharedString::default());
                win.set_has_selection(false);
                win.set_mapping_active(false);
            }
        });
        timer
    };

    let _ = refresh;
    window
        .run()
        .map_err(|e| crate::error::AppError::Message(format!("the user interface failed: {e}")))?;
    Ok(())
}
