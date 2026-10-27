//! The app's consent to write. This is the only place that constructs a
//! `WriteConsent`; it is used only by the apply steps (and the restore's
//! prepare step, whose comparison switches profiles), each after the user
//! confirmed the plan shown in the app. A source-scan test enforces both.

use openwraith_hid::{ConsentOrigin, WriteConsent};

/// The user agreed in the app's confirmation dialog.
pub(crate) fn application() -> WriteConsent {
    WriteConsent::new(ConsentOrigin::Application)
}
