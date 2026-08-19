//! Background controller discovery monitor.
//!
//! A dedicated thread rescans `/dev/input` on an interval and pushes each
//! snapshot through a channel. The UI drains the channel on its event loop
//! timer, keeping the GUI responsive and free of blocking I/O.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::controllers::discovery::{scan_controllers, DiscoveredController};

pub type ControllerSnapshot = Vec<DiscoveredController>;

/// Sends `ControllerSnapshot`s on `tx` until dropped or the receiver is gone.
pub struct ControllerMonitor {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ControllerMonitor {
    pub fn spawn(tx: Sender<ControllerSnapshot>, interval: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();

        let thread = thread::Builder::new()
            .name("xxmapper-monitor".into())
            .spawn(move || {
                while !stop_flag.load(Ordering::Relaxed) {
                    let snapshot = scan_controllers().unwrap_or_default();
                    if tx.send(snapshot).is_err() {
                        break;
                    }
                    // Sleep in small steps so a stop request is honoured
                    // promptly while still only rescannning every `interval`.
                    let mut waited = Duration::ZERO;
                    while waited < interval && !stop_flag.load(Ordering::Relaxed) {
                        let step = std::cmp::min(interval - waited, Duration::from_millis(100));
                        thread::sleep(step);
                        waited += step;
                    }
                }
            })
            .expect("failed to spawn controller monitor thread");

        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for ControllerMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn monitor_pushes_snapshots_and_stops() {
        let (tx, rx) = mpsc::channel();
        let monitor = ControllerMonitor::spawn(tx, Duration::from_millis(10));

        // The very first snapshot must arrive promptly. It may be empty when no
        // controller is connected, or contain discovered controllers when one
        // is present; either way the loop is proven to run and deliver.
        let _snapshot = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("monitor must send its first snapshot");

        drop(monitor);
        assert!(
            rx.recv_timeout(Duration::from_secs(1)).is_err(),
            "monitor must stop after drop"
        );
    }
}
