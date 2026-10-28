//! The app's consent to write. This is the only place that constructs a
//! `WriteConsent`; the application's consent is used only by the apply
//! steps (and the restore's prepare step, whose comparison switches
//! profiles), each after the user confirmed the plan shown in the app. The
//! standing rule's consent (a per-game rule the user created) is used only
//! by `ops/auto_profile.rs`, to check that the library's profile switcher
//! switches with it and nothing else. A source-scan test enforces all of it.

use openwraith_hid::{ConsentOrigin, WriteConsent};

/// The user agreed in the app's confirmation dialog.
pub(crate) fn application() -> WriteConsent {
    WriteConsent::new(ConsentOrigin::Application)
}

/// The user created a rule beforehand (a per-game automatic profile): the
/// library's [`ProfileSwitcher`](openwraith_hid::ProfileSwitcher) makes its
/// switches with this consent, which covers profile selects only.
pub(crate) fn standing_rule() -> WriteConsent {
    WriteConsent::new(ConsentOrigin::StandingRule)
}
