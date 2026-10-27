//! The core's sinks over Tauri channels and the app handle.

use openwraith_app_core::dto::{AppEvent, ProgressEvent};
use openwraith_app_core::{EventSink, ExitHook, ProgressSink};
use tauri::ipc::Channel;
use tauri::{AppHandle, Runtime};

/// Status changes and notices for the `subscribe` channel. Send errors are
/// ignored (a reloaded window subscribes again).
pub struct ChannelEvents(pub Channel<AppEvent>);

impl EventSink for ChannelEvents {
    fn send(&self, event: AppEvent) {
        let _ = self.0.send(event);
    }
}

/// The progress of one operation.
pub struct ChannelProgress(pub Channel<ProgressEvent>);

impl ProgressSink for ChannelProgress {
    fn send(&self, event: ProgressEvent) {
        let _ = self.0.send(event);
    }
}

/// Exits the app after an operation (the user chose "stop safely and
/// close").
pub struct AppExit<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> ExitHook for AppExit<R> {
    fn exit(&self) {
        self.0.exit(0);
    }
}
