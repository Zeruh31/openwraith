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

/// Applies changed background settings to the shell: the tray icon and the
/// per-user Run entry (the glue's `autostart.rs`), and says what Windows
/// will do at the next sign-in. Implemented by the Tauri glue and
/// registered with
/// [`WorkerHandle::set_shell_hook`](crate::WorkerHandle::set_shell_hook);
/// without one, the changes are only reported in the result of
/// `set_background_settings` and the start state is `unavailable`. Tests
/// use a recording hook: nothing here touches the real tray or Run key.
pub trait ShellHook: Send + Sync {
    /// Applies `changes` (only the `Some` fields changed). An error is
    /// reported to the frontend; the settings stay saved.
    fn apply(&self, changes: &crate::dto::ShellChanges) -> Result<(), crate::dto::AppError>;

    /// What Windows will do at the next sign-in, read now; `wanted` is
    /// whether the saved settings want OpenWraith to start with Windows.
    /// Asked for every `get_background_settings` and after every
    /// `set_background_settings`; reads only.
    fn windows_start(&self, wanted: bool) -> crate::dto::WindowsStartState;
}

/// Lists the running programs for Settings > Games ("add from running
/// programs"). Implemented by the Tauri glue (`winproc.rs`: process names
/// and paths only) and registered with
/// [`WorkerHandle::set_program_source`](crate::WorkerHandle::set_program_source);
/// without one the list is empty. Tests use a fixed list.
pub trait ProgramSource: Send + Sync {
    /// The running programs (any order; the core filters and sorts).
    fn running_programs(&self) -> Vec<crate::shell::RunningProgram>;
}

/// Shows OS notifications. Implemented by the Tauri glue
/// (`tauri-plugin-notification`) and registered with
/// [`WorkerHandle::set_notification_sink`](crate::WorkerHandle::set_notification_sink).
/// The core has already applied the user's notification settings and the
/// "one per kind per 10 minutes" rule. Tests record them: nothing here shows
/// a real notification.
pub trait NotificationSink: Send + Sync {
    /// Shows one notification.
    fn show(&self, notification: &crate::shell::OsNotification);
}

/// Shows the tray icon and its menu. Implemented by the Tauri glue
/// (`tray-icon`) and registered with
/// [`WorkerHandle::set_tray_sink`](crate::WorkerHandle::set_tray_sink); called
/// with the new model whenever it changes (and once at registration).
pub trait TraySink: Send + Sync {
    /// Shows `model`.
    fn update(&self, model: &crate::shell::TrayModel);
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

    impl super::NotificationSink for Recorder<crate::shell::OsNotification> {
        fn show(&self, notification: &crate::shell::OsNotification) {
            if let Ok(mut v) = self.items.lock() {
                v.push(notification.clone());
            }
        }
    }

    impl super::TraySink for Recorder<crate::shell::TrayModel> {
        fn update(&self, model: &crate::shell::TrayModel) {
            if let Ok(mut v) = self.items.lock() {
                v.push(model.clone());
            }
        }
    }

    /// A fixed list of running programs.
    #[derive(Debug, Default)]
    pub(crate) struct FixedPrograms(pub(crate) Vec<crate::shell::RunningProgram>);

    impl super::ProgramSource for FixedPrograms {
        fn running_programs(&self) -> Vec<crate::shell::RunningProgram> {
            self.0.clone()
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
