//! HID transport, read-only session and lossless backup for the Wraith W75.
//!
//! This crate connects the pure protocol implementation
//! ([`openwraith_protocol`], re-exported as [`protocol`]) to a real keyboard:
//!
//! * [`Transport`]: sends an [`OutPacket`](protocol::OutPacket) and receives
//!   input reports. [`HidTransport`] talks to a keyboard through hidapi;
//!   `MockTransport` (feature `mock`) replays the USB captures in
//!   `captures/`.
//! * [`Session`]: sends one [`ReadRequest`](protocol::ReadRequest) at a time,
//!   collects its reply chunks, ignores unrelated traffic, retries once on a
//!   timeout and stops if the keyboard reports a reset (spec section 2.8).
//! * [`read_all`]: reads every setting of the active onboard profile (plus
//!   the tables whose profile scope is unverified) into a [`Backup`] that
//!   keeps every reply raw.
//! * [`read_summary`]: the few reads behind a device overview
//!   ([`DeviceSummary`]).
//!
//! # Read-only by construction
//!
//! Phase 1 never changes anything on the keyboard. A transport can only send
//! an `OutPacket`, and the protocol crate builds those only from read
//! requests. There is no API in this crate that accepts raw bytes for
//! sending, and the hidapi handle inside [`HidTransport`] is private. A
//! session never answers the keyboard's reset notification.
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
mod error;
mod hid;
#[cfg(feature = "mock")]
pub mod mock;
mod session;
mod summary;
mod transport;

pub use openwraith_protocol as protocol;

pub use backup::{
    BACKUP_EXTENSION, BACKUP_FORMAT, BACKUP_SCHEMA_VERSION, BACKUP_SCOPE, Backup, BackupEntry,
    BackupOptions, DeviceIdentity, Progress, ToolInfo, read_all, read_all_with, rfc3339_utc,
};
pub use error::{BackupError, SessionError, TransportError};
pub use hid::{DeviceDescriptor, HidBackend, HidTransport, is_supported_interface};
#[cfg(feature = "mock")]
pub use mock::{MockError, MockTransport};
pub use session::{Session, SessionOptions, SessionStats};
pub use summary::{DeviceSummary, read_summary};
pub use transport::{Transport, UsbInfo};
