//! Finding and opening the keyboard.
//!
//! The read-only commands open a [`HidTransport`], which cannot send a write.
//! Only the commands that write (through the guarded write path of
//! `openwraith-hid`) open a [`WritableHidTransport`], through a
//! [`Connector`] so that their logic can be tested against the device
//! simulator. The connector also says where the device lock shared with the
//! desktop app lives ([`Connector::lock_dir`]).

use std::path::PathBuf;

use openwraith_hid::coordination::{self, LockError};
use openwraith_hid::{
    DeviceDescriptor, HidBackend, HidTransport, Session, Transport, WritableHidTransport,
    WriteTransport,
};

/// Problems choosing a device.
#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    /// No supported configuration interface is connected.
    #[error("no supported keyboard found")]
    NotFound,
    /// Several supported keyboards are connected and none was chosen.
    #[error("several supported keyboards are connected:\n{list}choose one with --device <PATH>")]
    Ambiguous {
        /// One line per device.
        list: String,
    },
    /// `--device` names a path that is not a connected supported keyboard.
    #[error("no connected supported keyboard has the path {0:?} (see `openwraith list`)")]
    UnknownPath(String),
}

/// The configuration interfaces of every connected supported keyboard.
/// Enumerates only; nothing is opened.
pub fn list() -> anyhow::Result<Vec<DeviceDescriptor>> {
    Ok(HidBackend::new()?.devices())
}

/// One line describing a device, for listings.
pub fn describe(device: &DeviceDescriptor) -> String {
    let usb = &device.usb;
    let mut text = format!("{:04x}:{:04x}", usb.vendor_id, usb.product_id);
    if let Some(product) = &usb.product {
        text.push_str(&format!(" \"{product}\""));
    }
    if device.interface_number >= 0 {
        text.push_str(&format!(", interface {}", device.interface_number));
    }
    if let Some(serial) = &usb.serial {
        text.push_str(&format!(", serial {serial}"));
    }
    text
}

/// Opens a read-only session with the chosen keyboard: the one at `path`, or
/// the only one connected.
pub fn open_session(path: Option<&str>) -> anyhow::Result<Session<HidTransport>> {
    let backend = HidBackend::new()?;
    let device = choose(&backend, path)?;
    Ok(Session::new(backend.open(&device)?))
}

/// Opens a session that can write (Phase 2) with the chosen keyboard. Only
/// the commands that write use it; writing still needs consent and passes
/// the "other program" gate of `Session::writer`.
pub fn open_writable_session(path: Option<&str>) -> anyhow::Result<Session<WritableHidTransport>> {
    let backend = HidBackend::new()?;
    let device = choose(&backend, path)?;
    Ok(Session::new(backend.open_writable(&device)?))
}

/// The keyboard at `path`, or the only one connected.
fn choose(backend: &HidBackend, path: Option<&str>) -> anyhow::Result<DeviceDescriptor> {
    let devices = backend.devices();
    let device = match path {
        Some(path) => devices
            .iter()
            .find(|d| d.path() == path)
            .ok_or_else(|| DeviceError::UnknownPath(path.to_string()))?,
        None => match devices.as_slice() {
            [] => return Err(DeviceError::NotFound.into()),
            [only] => only,
            many => {
                let list = many
                    .iter()
                    .map(|d| format!("  {}  ({})\n", d.path(), describe(d)))
                    .collect();
                return Err(DeviceError::Ambiguous { list }.into());
            }
        },
    };
    Ok(device.clone())
}

/// Opens sessions with the keyboard for the commands that may write.
///
/// [`HidConnector`] opens real HID devices. The CLI's unit tests use a
/// connector over the device simulator instead; the binary has no such
/// mechanism.
pub trait Connector {
    /// Transport of read-only sessions (cannot send a write).
    type Read: Transport;
    /// Transport of sessions that may write.
    type Write: WriteTransport;

    /// Opens a read-only session.
    fn open_read(&mut self) -> anyhow::Result<Session<Self::Read>>;

    /// Opens a session that may write.
    fn open_write(&mut self) -> anyhow::Result<Session<Self::Write>>;

    /// The directory of the device lock shared with the desktop app
    /// ([`openwraith_hid::coordination`]); `None` takes no lock (the unit
    /// tests' simulator, unless a test asks for one).
    fn lock_dir(&self) -> Result<Option<PathBuf>, LockError>;
}

/// [`Connector`] for real keyboards (`--device` or the only one connected).
#[derive(Debug, Clone, Default)]
pub struct HidConnector {
    device: Option<String>,
}

impl HidConnector {
    /// A connector for the keyboard at `device`, or the only one connected.
    pub fn new(device: Option<String>) -> Self {
        HidConnector { device }
    }
}

impl Connector for HidConnector {
    type Read = HidTransport;
    type Write = WritableHidTransport;

    fn open_read(&mut self) -> anyhow::Result<Session<HidTransport>> {
        open_session(self.device.as_deref())
    }

    fn open_write(&mut self) -> anyhow::Result<Session<WritableHidTransport>> {
        open_writable_session(self.device.as_deref())
    }

    /// The per-user lock the desktop app uses too
    /// (`%LOCALAPPDATA%\OpenWraith` on Windows).
    fn lock_dir(&self) -> Result<Option<PathBuf>, LockError> {
        coordination::default_dir().map(Some)
    }
}
