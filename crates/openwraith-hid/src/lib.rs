//! HID transport, session, lossless backup, and the guarded write path for
//! the Wraith W75.
//!
//! This crate connects the pure protocol implementation
//! ([`openwraith_protocol`], re-exported as [`protocol`]) to a real keyboard:
//!
//! * [`Transport`]: sends an [`OutPacket`](protocol::OutPacket) (a read
//!   request) and receives input reports. [`HidTransport`] talks to a
//!   keyboard through hidapi; `MockTransport` (feature `mock`) replays the
//!   USB captures in `captures/`.
//! * [`Session`]: sends one [`ReadRequest`](protocol::ReadRequest) at a time,
//!   collects its reply chunks, ignores unrelated traffic, retries once on a
//!   timeout and stops if the keyboard reports a reset (spec section 2.8).
//! * [`read_all`]: reads every setting of the active onboard profile (plus
//!   the tables whose profile scope is unverified) into a [`Backup`] that
//!   keeps every reply raw.
//! * [`read_summary`]: the few reads behind a device overview
//!   ([`DeviceSummary`]).
//!
//! Phase 2 adds a separate, guarded write path:
//!
//! * [`WriteTransport`]: the extension trait that can send a
//!   [`WritePacket`](protocol::WritePacket). [`WritableHidTransport`]
//!   (opened with [`HidBackend::open_writable`]) and the simulator
//!   `DeviceSim` (feature `mock`) implement it.
//! * [`Writer`] ([`Session::writer`]): consent, the "other program" gate,
//!   profile selection, and [`Writer::write_guarded`], the only public table
//!   write (with [`Writer::write_commit_trigger`], its variant that re-sends
//!   an unchanged Fn keymap after a base keymap): it needs a [`SavedBackup`]
//!   of the active profile and writes stop-and-wait with read-back
//!   verification. The keyboard saves a write about 1 s later, so the writer
//!   never switches profiles (and operations never return) before the
//!   [`CommitSettle`] has passed; it keeps a base keymap only when the Fn
//!   keymap is written after it ([`CommitTrigger`]).
//! * [`read_profiles`]: an all-profile backup (schema 2).
//! * [`safety`]: safety backups saved and verified before any write
//!   ([`SavedBackup`]).
//! * [`restore`]: plan, prepare and execute a restore (also importing one
//!   exported profile into another, [`Backup::export_profile`]).
//! * [`edit`]: edits of the active profile: triggers (and
//!   [`presets`]), the main light, key assignments on both layers, macros
//!   and advanced keys (one keymap-group write, in the vendor site's order),
//!   per-key colours, the dead band and SOCD. Each plans read-only, then
//!   writes through the writer, waits for the save and, where the keyboard's
//!   keeping a table is not proven on hardware, checks it by switching
//!   profiles away and back.
//! * [`travel`]: the live key-depth view (travel test) behind a guard that
//!   always closes it.
//! * [`ProfileSwitcher`] ([`Session::profile_switcher`]): profile selects
//!   for standing rules (an automatic profile per game), nothing else.
//!
//! For the desktop app and the command line tool side by side:
//!
//! * [`known_good`]: reset detection against known-good (automatic)
//!   backups ([`assess`], [`reset_verdict`]) and [`compose`], which builds
//!   one all-profile backup from per-profile ones for a restore. Pure:
//!   nothing is read, written or restored by itself.
//! * [`coordination`]: [`DeviceLock`], the per-user lock file that the
//!   programs writing to the keyboard hold exclusively and the ones reading
//!   it hold shared.
//!
//! Reports the keyboard sends on its own (the SOCD switch on Fn + Right
//! Shift, the key-depth stream) are never taken for another program; a
//! key-depth stream that no test of the session opened refuses every table
//! write until it is closed. Every write packet passes an egress allowlist
//! ([`is_permitted_packet`](protocol::is_permitted_packet)) right before it
//! is sent.
//!
//! # Read-only paths stay read-only
//!
//! A plain [`Transport`] can only send an `OutPacket`, and the protocol crate
//! builds those only from read requests. [`read_all`], [`read_summary`] and
//! [`dry_run_restore`] are generic over `Transport`, so they **cannot** write:
//! sending a write needs [`WriteTransport`], whose only packet type
//! (`WritePacket`) has no public constructor and is built only for the
//! write forms confirmed by captures and on hardware (W1-W11 and the
//! travel-test open and close). [`HidTransport`] does not implement
//! `WriteTransport`. There is no API in this crate that accepts raw bytes for
//! sending, and the hidapi handle is private. A session never answers the
//! keyboard's reset notification, and after one nothing more is written.
//!
//! # Example
//!
//! ```no_run
//! use openwraith_hid::{HidBackend, Session, read_all};
//!
//! let backend = HidBackend::new()?;
//! let device = backend.devices().into_iter().next().expect("no keyboard found");
//! let mut session = Session::new(backend.open(&device)?);
//! let backup = read_all(&mut session)?;
//! backup.save(std::path::Path::new(&backup.default_file_name()), false)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod backup;
pub mod coordination;
pub mod edit;
mod error;
mod group;
mod hid;
pub mod known_good;
#[cfg(feature = "mock")]
pub mod mock;
mod persist;
pub mod presets;
pub mod profiles;
pub mod restore;
pub mod safety;
mod session;
#[cfg(feature = "mock")]
pub mod sim;
mod summary;
mod switcher;
mod transport;
pub mod travel;
pub mod writer;

pub use openwraith_protocol as protocol;

pub use backup::{
    BACKUP_EXTENSION, BACKUP_FORMAT, BACKUP_MAX_FILE_BYTES, BACKUP_SCHEMA_MAX,
    BACKUP_SCHEMA_VERSION, BACKUP_SCHEMA_VERSION_PROFILES, BACKUP_SCOPE, BACKUP_SCOPE_EXPORT,
    BACKUP_SCOPE_PROFILES, Backup, BackupEntry, BackupOptions, DeviceIdentity, ProfileSnapshot,
    Progress, SnapshotView, ToolInfo, read_all, read_all_cancellable, read_all_observed,
    read_all_with, rfc3339_utc,
};
pub use coordination::{DeviceLock, LockError, LockInfo, LockMode, LockState};
pub use edit::{
    AdvancedSlotView, ColorChange, ColorEdit, ColorEditPlan, CommitTriggerMode, DeadBandChange,
    DeadBandEdit, DeadBandEditPlan, EditError, EditOptions, EditReport, EditWarning,
    EntryChangeView, KeyEdit, KeymapChange, KeymapEdit, KeymapEditPlan, KeymapGroupState,
    LightEdit, LightEditPlan, MacroSlotView, PersistenceCheckMode, PlannedEditWrite, RtEdit,
    SlotChangeView, SlotKind, SocdEdit, SocdEditPlan, SocdState, TriggerEdit, TriggerEditPlan,
    apply_color_edit, apply_dead_band_edit, apply_keymap_edit, apply_light_edit, apply_socd_edit,
    apply_trigger_edit, persistence_check_tables, plan_color_edit, plan_dead_band_edit,
    plan_keymap_edit, plan_keymap_edit_on, plan_light_edit, plan_socd_edit, plan_trigger_edit,
};
pub use error::{
    BackupError, ErrorInfo, ErrorKind, ListenError, ProfilesError, ProfilesFailure, RestoreError,
    SessionError, TransportError, WriteError,
};
pub use group::{ClearedMacro, DanglingBinding, ResendReason, SlotRef};
pub use hid::{
    DeviceChoiceError, DeviceDescriptor, DeviceListing, HidBackend, HidTransport,
    WritableHidTransport, choose_device, is_supported_interface,
};
pub use known_good::{
    FirmwareId, KnownGoodError, KnownGoodTable, ProfileAssessment, ProfileEvidence, ResetEvidence,
    ResetVerdict, TableAssessment, TableVerdict, assess, compose, reset_verdict,
};
#[cfg(feature = "mock")]
pub use mock::{MockError, MockTransport};
pub use presets::{TRIGGER_PRESETS, TriggerPreset, preset_edit, trigger_preset};
pub use profiles::{
    ProfileSet, ProfilesOptions, ProfilesProgress, read_all_profiles, read_profiles,
};
pub use restore::{
    BlockCause, CommitTriggerReason, EntryChange, FinalCheck, FinalMismatch, ImportOptions,
    ImportSelection, ItemReport, ItemResult, Outcome, Persistence, PersistenceCheck, PlanAction,
    PlanItem, PlanWarning, PlannedWrite, PreparedRestore, ProfilePlan, ProfileReport,
    RestoreOptions, RestorePlan, RestoreReport, SkipReason, WriteProgress, dry_run_restore,
    execute_restore, plan_restore, prepare_restore,
};
pub use safety::SavedBackup;
pub use session::{
    ForeignActivity, ListenSummary, Session, SessionOptions, SessionStats, TravelState,
};
#[cfg(feature = "mock")]
pub use sim::{DeviceSim, KeymapCommit, Scope, ScopeConfig, SimPacket, SimTable, Violation};
pub use summary::{DeviceSummary, read_summary};
pub use switcher::ProfileSwitcher;
pub use transport::{Transport, UsbInfo, WriteTransport};
pub use travel::{
    CloseResult, TravelFrame, TravelOptions, TravelOutcome, TravelStop, TravelTest, live_travel,
};
pub use writer::{
    CommitSettle, CommitTrigger, CommitWait, ConsentOrigin, GuardedWrite, PacketProgress,
    SelectOutcome, WriteConsent, WriteOptions, WriteOutcome, WriteWarning, Writer,
};
