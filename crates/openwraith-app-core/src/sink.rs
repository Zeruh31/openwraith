//! Where events and progress go: the app crate implements these over Tauri
//! channels, the tests record them.

use crate::dto::{AppEvent, DepthFrame, ProgressEvent};

/// Receives status changes and notices (the `subscribe` channel). Send
/// errors are ignored by the implementation (a reloaded window).
pub trait EventSink: Send + Sync {
    /// Delivers one event.
    fn send(&self, event: AppEvent);
}

/// Receives the progress of one operation.
pub trait ProgressSink: Send + Sync {
    /// Delivers one event.
    fn send(&self, event: ProgressEvent);
}

/// Receives the frames of the live key-depth view.
pub trait DepthSink: Send + Sync {
    /// Delivers one frame; `false` when nobody can receive frames any more
    /// (the view then stops and the test is closed).
    fn send(&self, frame: DepthFrame) -> bool;
}

/// Exits the app (after an operation finished, when the user chose "stop
/// safely and close").
pub trait ExitHook: Send + Sync {
    /// Exits.
    fn exit(&self);
}

/// A progress sink that drops everything.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NoProgress;

impl ProgressSink for NoProgress {
    fn send(&self, _event: ProgressEvent) {}
}

#[cfg(test)]
pub(crate) mod recorder {
    //! Sinks that record what they receive (tests).

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{DepthSink, EventSink, ExitHook, ProgressSink};
    use crate::dto::{AppEvent, DepthFrame, ProgressEvent};

    /// Records events.
    #[derive(Debug)]
    pub(crate) struct Recorder<T> {
        pub(crate) items: Mutex<Vec<T>>,
    }

    impl<T> Default for Recorder<T> {
        fn default() -> Self {
            Recorder {
                items: Mutex::new(Vec::new()),
            }
        }
    }

    impl<T: Clone> Recorder<T> {
        /// Everything recorded so far.
        pub(crate) fn all(&self) -> Vec<T> {
            self.items.lock().map(|v| v.clone()).unwrap_or_default()
        }

        /// Forgets everything recorded so far.
        pub(crate) fn clear(&self) {
            if let Ok(mut v) = self.items.lock() {
                v.clear();
            }
        }
    }

    impl EventSink for Recorder<AppEvent> {
        fn send(&self, event: AppEvent) {
            if let Ok(mut v) = self.items.lock() {
                v.push(event);
            }
        }
    }

    impl ProgressSink for Recorder<ProgressEvent> {
        fn send(&self, event: ProgressEvent) {
            if let Ok(mut v) = self.items.lock() {
                v.push(event);
            }
        }
    }

    impl DepthSink for Recorder<DepthFrame> {
        fn send(&self, frame: DepthFrame) -> bool {
            if let Ok(mut v) = self.items.lock() {
                v.push(frame);
            }
            true
        }
    }

    /// Counts exits.
    #[derive(Debug, Default)]
    pub(crate) struct ExitCounter(pub(crate) AtomicUsize);

    impl ExitHook for ExitCounter {
        fn exit(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}
