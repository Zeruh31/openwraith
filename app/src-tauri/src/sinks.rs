//! The core's sinks over Tauri channels and the app handle.

use openwraith_app_core::dto::{AppEvent, DepthFrame, ProgressEvent};
use openwraith_app_core::{DepthSink, EventSink, ExitHook, ProgressSink};
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

/// The frames of the live key-depth view. A send that fails (the window is
/// gone) stops the view; the test is closed.
pub struct ChannelDepth(pub Channel<DepthFrame>);

impl DepthSink for ChannelDepth {
    fn send(&self, frame: DepthFrame) -> bool {
        self.0.send(frame).is_ok()
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
