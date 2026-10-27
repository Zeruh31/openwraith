//! Profile switches made by a standing rule (an automatic profile per game):
//! [`ProfileSwitcher`].
//!
//! An automatic code path never writes a setting: the only thing it can do
//! is select a profile, and only when nothing is pending. A
//! [`ProfileSwitcher`] wraps a [`Writer`] created with
//! [`ConsentOrigin::StandingRule`] and exposes nothing but
//! [`switch_to`](ProfileSwitcher::switch_to):
//!
//! ```compile_fail
//! use openwraith_hid::{ProfileSwitcher, SavedBackup, WriteTransport};
//! use openwraith_hid::protocol::{ProfileId, WriteRequest};
//! fn write<T: WriteTransport>(switcher: &mut ProfileSwitcher<'_, T>, saved: &SavedBackup) {
//!     let request = WriteRequest::SelectProfile(ProfileId::new(1).unwrap());
//!     let _ = switcher.write_guarded(saved, &request);
//! }
//! ```
//!
//! ```compile_fail
//! use openwraith_hid::{CommitTrigger, ProfileSwitcher, SavedBackup, WriteTransport};
//! fn trigger<T: WriteTransport>(
//!     switcher: &mut ProfileSwitcher<'_, T>,
//!     saved: &SavedBackup,
//!     trigger: &CommitTrigger,
//! ) {
//!     let _ = switcher.write_commit_trigger(saved, trigger);
//! }
//! ```

use openwraith_protocol::ProfileId;

use crate::error::WriteError;
use crate::session::Session;
use crate::transport::WriteTransport;
use crate::writer::{CommitWait, ConsentOrigin, SelectOutcome, WriteConsent, WriteOptions, Writer};

/// Selects profiles for a standing rule; nothing else. Created by
/// [`Session::profile_switcher`].
pub struct ProfileSwitcher<'s, T: WriteTransport> {
    writer: Writer<'s, T>,
}

impl<T: WriteTransport> Session<T> {
    /// A [`ProfileSwitcher`] (consent [`ConsentOrigin::StandingRule`]) with
    /// the default [`WriteOptions`], after the same "other program" gate as
    /// [`writer`](Session::writer).
    pub fn profile_switcher(&mut self) -> Result<ProfileSwitcher<'_, T>, WriteError> {
        self.profile_switcher_with(WriteOptions::default())
    }

    /// [`profile_switcher`](Self::profile_switcher) with custom options.
    pub fn profile_switcher_with(
        &mut self,
        options: WriteOptions,
    ) -> Result<ProfileSwitcher<'_, T>, WriteError> {
        let writer = self.writer_with(WriteConsent::new(ConsentOrigin::StandingRule), options)?;
        Ok(ProfileSwitcher { writer })
    }
}

impl<T: WriteTransport> ProfileSwitcher<'_, T> {
    /// Selects `profile`, unless that could lose something or disturb an
    /// operation:
    ///
    /// * refuses with [`WriteError::BaseKeymapUnsaved`] while a base keymap
    ///   of this session waits for its Fn-keymap write (the switch would
    ///   drop it, spec 4.8.2);
    /// * refuses while a travel test may be open or the keyboard streams
    ///   from one ([`WriteError::TravelTestMaybeOpen`],
    ///   [`WriteError::TravelTestOpen`]);
    /// * refuses after another program was seen, a reset or a lost
    ///   connection.
    ///
    /// Otherwise it is [`Writer::select_profile_observed`]: it waits until
    /// the keyboard had the commit settle since the last write (`on_wait` is
    /// told), then selects and confirms.
    pub fn switch_to(
        &mut self,
        profile: ProfileId,
        on_wait: &mut dyn FnMut(CommitWait),
    ) -> Result<SelectOutcome, WriteError> {
        self.writer.ensure_usable()?;
        if let Some(evidence) = self.writer.foreign_activity_seen() {
            return Err(WriteError::OtherProgramActive { evidence });
        }
        if self.writer.session().base_keymap_unsaved() {
            return Err(WriteError::BaseKeymapUnsaved);
        }
        self.writer.check_travel_closed()?;
        self.writer.select_profile_observed(profile, on_wait)
    }

    /// The consent the switches are made with (always
    /// [`ConsentOrigin::StandingRule`]).
    pub fn consent(&self) -> WriteConsent {
        self.writer.consent()
    }

    /// The session (statistics and events).
    pub fn session(&self) -> &Session<T> {
        self.writer.session()
    }
}
