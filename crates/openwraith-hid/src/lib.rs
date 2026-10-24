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
//!   write: it needs a [`SavedBackup`] of the active profile and writes
//!   stop-and-wait with read-back verification.
//! * [`read_profiles`]: an all-profile backup (schema 2).
//! * [`safety`]: safety backups saved and verified before any write
//!   ([`SavedBackup`]).
//! * [`restore`]: plan, prepare and execute a restore.
//! * [`edit`]: trigger and main-light edits of the active profile.
//!
//! # Read-only paths stay read-only
//!
//! A plain [`Transport`] can only send an `OutPacket`, and the protocol crate
//! builds those only from read requests. [`read_all`], [`read_summary`] and
//! [`dry_run_restore`] are generic over `Transport`, so they **cannot** write:
//! sending a write needs [`WriteTransport`], whose only packet type
//! (`WritePacket`) has no public constructor and is built only for the nine
//! write forms confirmed on hardware. [`HidTransport`] does not implement
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
pub mod edit;
mod error;
mod hid;
#[cfg(feature = "mock")]
pub mod mock;
pub mod profiles;
pub mod restore;
pub mod safety;
mod session;
#[cfg(feature = "mock")]
pub mod sim;
mod summary;
mod transport;
pub mod writer;

pub use openwraith_protocol as protocol;

pub use backup::{
    BACKUP_EXTENSION, BACKUP_FORMAT, BACKUP_SCHEMA_MAX, BACKUP_SCHEMA_VERSION,
    BACKUP_SCHEMA_VERSION_PROFILES, BACKUP_SCOPE, BACKUP_SCOPE_PROFILES, Backup, BackupEntry,
    BackupOptions, DeviceIdentity, ProfileSnapshot, Progress, SnapshotView, ToolInfo, read_all,
    read_all_cancellable, read_all_with, rfc3339_utc,
};
pub use edit::{
    EditReport, KeyEdit, LightEdit, LightEditPlan, RtEdit, TriggerEdit, TriggerEditPlan,
    apply_light_edit, apply_trigger_edit, plan_light_edit, plan_trigger_edit,
};
pub use error::{
    BackupError, ErrorInfo, ErrorKind, ListenError, ProfilesError, ProfilesFailure, RestoreError,
    SessionError, TransportError, WriteError,
};
pub use hid::{
    DeviceDescriptor, HidBackend, HidTransport, WritableHidTransport, is_supported_interface,
};
#[cfg(feature = "mock")]
pub use mock::{MockError, MockTransport};
pub use profiles::{
    ProfileSet, ProfilesOptions, ProfilesProgress, read_all_profiles, read_profiles,
};
pub use restore::{
    BlockCause, EntryChange, FinalCheck, FinalMismatch, ItemReport, ItemResult, Outcome,
    PlanAction, PlanItem, PlanWarning, PlannedWrite, PreparedRestore, ProfilePlan, ProfileReport,
    RestoreOptions, RestorePlan, RestoreReport, SkipReason, WriteProgress, dry_run_restore,
    execute_restore, plan_restore, prepare_restore,
};
pub use safety::SavedBackup;
pub use session::{ForeignActivity, ListenSummary, Session, SessionOptions, SessionStats};
#[cfg(feature = "mock")]
pub use sim::{DeviceSim, Scope, ScopeConfig, SimPacket, SimTable, Violation};
pub use summary::{DeviceSummary, read_summary};
pub use transport::{Transport, UsbInfo, WriteTransport};
pub use writer::{
    ConsentOrigin, GuardedWrite, PacketProgress, SelectOutcome, WriteConsent, WriteOptions,
    WriteOutcome, WriteWarning, Writer,
};
