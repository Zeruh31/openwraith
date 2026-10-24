//! Finding and opening the keyboard.

use openwraith_hid::{DeviceDescriptor, HidBackend, HidTransport, Session};

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
    Ok(Session::new(backend.open(device)?))
}
