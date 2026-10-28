//! The live key-depth view (the keyboard's travel test) and the state of
//! the test on the keyboard.

use serde::{Deserialize, Serialize};

/// One frame of the live view (at most 60 per second; only frames with
/// movement are sent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DepthFrame {
    /// Increases with every frame.
    pub seq: u32,
    /// Milliseconds since the test was opened.
    pub open_ms: u32,
    /// The keys that moved since the last frame, in index order.
    pub keys: Vec<DepthSample>,
    /// Reports the library dropped since the last frame (they came faster
    /// than they were drained).
    pub dropped: u32,
}

/// The depth of one key in a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DepthSample {
    /// The key index.
    pub index: u8,
    /// The latest depth, in trigger units (0 at rest).
    pub depth: u16,
    /// The deepest depth since the last frame, in trigger units.
    pub peak: u16,
}

/// Why the frontend stops the live view (`stop_depth_view`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum DepthStopRequest {
    /// The user pressed Stop.
    Requested,
    /// The window was hidden or minimised.
    WindowHidden,
    /// The user left the tab or the page.
    PageLeft,
}

/// Why the live view stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum DepthStopReason {
    /// The user pressed Stop.
    Requested,
    /// The window was hidden or minimised.
    WindowHidden,
    /// The user left the tab or the page.
    PageLeft,
    /// The app is closing.
    AppExit,
    /// The connection to the keyboard was lost.
    Disconnected,
    /// The keyboard reset itself (nothing was sent after it, not even the
    /// close).
    KeyboardReset,
    /// Another program talked to the keyboard (the test was closed).
    OtherProgram,
    /// The keyboard switched profiles (Fn + 1..4; the test was closed).
    ProfileChanged,
    /// The window that showed the view is gone (reloaded, or its channel
    /// closed).
    ChannelClosed,
    /// No key moved for 5 minutes ("Paused").
    NoMovement,
    /// The view ran for its longest time (60 minutes).
    TimeLimit,
}

/// How the close of the test went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "close", rename_all = "snake_case")]
pub enum TravelCloseView {
    /// The keyboard acknowledged the close: the test is off.
    Acked,
    /// The close was sent twice without an acknowledgement: the keyboard
    /// may still stream (writes are refused until a close is acknowledged).
    NotAcked,
    /// No close was sent (a reset or a lost connection).
    NotSent {
        /// Why (technical English).
        reason: String,
    },
}

/// What a live view did (`start_depth_view` returns it when it stopped).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DepthViewReport {
    /// Why it stopped.
    pub reason: DepthStopReason,
    /// How the close went.
    pub close: TravelCloseView,
    /// Stream reports received.
    pub reports: u32,
    /// Stream reports dropped.
    pub dropped: u32,
    /// How long the test was open, in milliseconds.
    pub open_ms: u32,
    /// Whether the test may still be open on the keyboard ("Stop it"
    /// offers `close_stray_travel_test`).
    pub may_be_open: bool,
}

/// Whether a key-depth test is (or may be) open on the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TravelStatusView {
    /// The session's view of the test.
    pub state: TravelStateView,
    /// Stream reports that arrived while no view of this app was open (a
    /// test another run left open). Table writes are refused while not 0.
    pub stray_reports: u32,
    /// The marker of a live view of an earlier run was still there at the
    /// start: the keyboard may still be streaming.
    pub marker_left: bool,
}

/// The session's view of the travel test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TravelStateView {
    /// No test of this app is open.
    Closed,
    /// The live view runs.
    Open,
    /// A close was not acknowledged: the keyboard may still stream; table
    /// writes are refused until a close is acknowledged.
    MaybeOpen,
}

/// What `close_stray_travel_test` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TravelCloseReport {
    /// How the close went.
    pub close: TravelCloseView,
    /// Stray stream reports seen before the close (0: no key moved, or no
    /// test was open; the close is harmless either way).
    pub stray_reports: u32,
}
