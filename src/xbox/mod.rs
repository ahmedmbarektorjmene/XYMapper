//! Xbox virtual-controller backends.
//!
//! The primary implementation is the native `uinput` backend that opens
//! `/dev/uinput` directly and presents itself as an Xbox 360 pad. `xboxdrv`
//! and the old Python/Bash scripts are never invoked at runtime.

pub mod emulator;
pub use emulator::{UinputXboxBackend, XboxBackend};