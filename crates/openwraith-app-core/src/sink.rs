//! Where events and progress go: the app crate implements these over Tauri
//! channels, the tests record them.

use crate::dto::{AppEvent, ProgressEvent};

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

    use super::{EventSink, ExitHook, ProgressSink};
    use crate::dto::{AppEvent, ProgressEvent};

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

    /// Counts exits.
    #[derive(Debug, Default)]
    pub(crate) struct ExitCounter(pub(crate) AtomicUsize);

    impl ExitHook for ExitCounter {
        fn exit(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}
