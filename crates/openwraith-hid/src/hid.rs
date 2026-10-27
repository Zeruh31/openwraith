//! Real keyboards through [hidapi](https://github.com/libusb/hidapi)
//! (spec sections 2.1 and 2.2).
//!
//! A Wraith keyboard enumerates several HID interfaces (keyboard, consumer
//! control, ...). Only the vendor configuration collection (usage page
//! `0xFF1B`, usage `0x91`) speaks the configuration protocol; every other
//! interface is ignored. Listing devices never opens them.

use std::ffi::CString;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};
use openwraith_protocol::{
    CONFIG_USAGE, CONFIG_USAGE_PAGE, OutPacket, PRODUCT_IDS, REPORT_LEN, Report, VENDOR_ID,
    WritePacket,
};

use crate::error::{ErrorKind, TransportError};
use crate::transport::{Transport, UsbInfo, WriteTransport};

/// Whether a HID interface with these identifiers is the configuration
/// interface of a supported keyboard.
pub fn is_supported_interface(
    vendor_id: u16,
    product_id: u16,
    usage_page: u16,
    usage: u16,
) -> bool {
    vendor_id == VENDOR_ID
        && PRODUCT_IDS.contains(&product_id)
        && usage_page == CONFIG_USAGE_PAGE
        && usage == CONFIG_USAGE
}

/// A supported configuration interface found during enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceDescriptor {
    path: CString,
    /// USB identity of the device.
    pub usb: UsbInfo,
    /// USB interface number (2 on Windows for the W75), or -1 if unknown.
    pub interface_number: i32,
    /// HID usage page of the collection (`0xFF1B`).
    pub usage_page: u16,
    /// HID usage of the collection (`0x91`).
    pub usage: u16,
}

impl DeviceDescriptor {
    /// Platform path of the HID interface, as text (for display and for the
    /// CLI's `--device` option).
    pub fn path(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    /// A serialisable description of this interface (for a device list in a
    /// user interface). Opening a device still needs the descriptor itself.
    pub fn listing(&self) -> DeviceListing {
        DeviceListing {
            path: self.path(),
            usb: self.usb.clone(),
            interface_number: self.interface_number,
        }
    }
}

/// A serialisable description of a supported configuration interface found
/// during enumeration ([`DeviceDescriptor::listing`]). It cannot be opened:
/// [`HidBackend::open`] takes the [`DeviceDescriptor`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceListing {
    /// Platform path of the HID interface, as text.
    pub path: String,
    /// USB identity of the device.
    pub usb: UsbInfo,
    /// USB interface number, or -1 if unknown.
    pub interface_number: i32,
}

/// Why [`choose_device`] found no device.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeviceChoiceError {
    /// No supported configuration interface is connected.
    #[error("no supported keyboard found")]
    NotFound,
    /// Several supported keyboards are connected and none was chosen.
    #[error("{count} supported keyboards are connected; choose one")]
    Ambiguous {
        /// How many are connected.
        count: usize,
    },
    /// The requested path is not a connected supported keyboard.
    #[error("no connected supported keyboard has the path {path:?}")]
    UnknownPath {
        /// The requested path.
        path: String,
    },
}

impl DeviceChoiceError {
    /// The machine-readable class of this error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            DeviceChoiceError::NotFound | DeviceChoiceError::UnknownPath { .. } => {
                ErrorKind::NotFound
            }
            DeviceChoiceError::Ambiguous { .. } => ErrorKind::InvalidRequest,
        }
    }
}

/// The only-one-device rule: the device at `path` when one is given, else
/// the only device in `devices`. Several devices without a path are
/// [`DeviceChoiceError::Ambiguous`]: a keyboard is never picked at random.
/// Opens nothing.
pub fn choose_device<'a>(
    devices: &'a [DeviceDescriptor],
    path: Option<&str>,
) -> Result<&'a DeviceDescriptor, DeviceChoiceError> {
    match path {
        Some(path) => devices.iter().find(|d| d.path() == path).ok_or_else(|| {
            DeviceChoiceError::UnknownPath {
                path: path.to_string(),
            }
        }),
        None => match devices {
            [] => Err(DeviceChoiceError::NotFound),
            [only] => Ok(only),
            many => Err(DeviceChoiceError::Ambiguous { count: many.len() }),
        },
    }
}

/// Entry point to the platform's HID subsystem.
pub struct HidBackend {
    api: HidApi,
}

impl HidBackend {
    /// Initialises hidapi and enumerates the HID devices (nothing is opened).
    pub fn new() -> Result<Self, TransportError> {
        let api = HidApi::new().map_err(|e| TransportError::Init(e.to_string()))?;
        Ok(HidBackend { api })
    }

    /// Re-enumerates the HID devices.
    pub fn refresh(&mut self) -> Result<(), TransportError> {
        self.api
            .refresh_devices()
            .map_err(|e| TransportError::Init(e.to_string()))
    }

    /// The configuration interfaces of every connected supported keyboard,
    /// sorted by path. Does not open anything.
    pub fn devices(&self) -> Vec<DeviceDescriptor> {
        let mut found: Vec<DeviceDescriptor> = self
            .api
            .device_list()
            .filter(|d| {
                is_supported_interface(d.vendor_id(), d.product_id(), d.usage_page(), d.usage())
            })
            .map(|d| DeviceDescriptor {
                path: d.path().to_owned(),
                usb: UsbInfo {
                    vendor_id: d.vendor_id(),
                    product_id: d.product_id(),
                    product: non_empty(d.product_string()),
                    manufacturer: non_empty(d.manufacturer_string()),
                    serial: non_empty(d.serial_number()),
                },
                interface_number: d.interface_number(),
                usage_page: d.usage_page(),
                usage: d.usage(),
            })
            .collect();
        found.sort_by(|a, b| a.path.cmp(&b.path));
        // Some backends list a collection twice; keep one entry per path.
        found.dedup_by(|a, b| a.path == b.path);
        found
    }

    /// Opens the configuration interface described by `device`.
    pub fn open(&self, device: &DeviceDescriptor) -> Result<HidTransport, TransportError> {
        let handle = self
            .api
            .open_path(&device.path)
            .map_err(|e| TransportError::Open {
                path: device.path(),
                message: e.to_string(),
            })?;
        Ok(HidTransport {
            device: handle,
            descriptor: device.clone(),
        })
    }

    /// Opens the configuration interface described by `device` for writing
    /// (Phase 2): the returned transport implements [`WriteTransport`] as
    /// well as [`Transport`]. Use [`open`](Self::open) for read-only work.
    pub fn open_writable(
        &self,
        device: &DeviceDescriptor,
    ) -> Result<WritableHidTransport, TransportError> {
        self.open(device).map(WritableHidTransport)
    }
}

/// An open configuration interface of a real keyboard.
///
/// The underlying hidapi handle is private: the only way to send something is
/// [`Transport::write`] with an [`OutPacket`], which can only be a read
/// request.
pub struct HidTransport {
    device: HidDevice,
    descriptor: DeviceDescriptor,
}

impl HidTransport {
    /// The descriptor this transport was opened from.
    pub fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }
}

impl Transport for HidTransport {
    fn write(&mut self, packet: &OutPacket) -> Result<(), TransportError> {
        // hidapi expects the report ID first: 0x01 + 63 payload bytes.
        self.device
            .write(&packet.to_hid_report())
            .map(drop)
            .map_err(|e| TransportError::Write(e.to_string()))
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Report>, TransportError> {
        // Room for the report ID, the payload and one spare byte so that an
        // oversized report is detected instead of silently truncated.
        let mut buf = [0u8; REPORT_LEN + 2];
        let n = self
            .device
            .read_timeout(&mut buf, timeout_millis(timeout))
            .map_err(|e| read_error(&e.to_string()))?;
        if n == 0 {
            return Ok(None);
        }
        // Handles both forms: with and without the leading report ID.
        let report = Report::from_hid_buffer(buf.get(..n).unwrap_or_default())?;
        Ok(Some(report))
    }

    fn usb_info(&self) -> Option<UsbInfo> {
        Some(self.descriptor.usb.clone())
    }
}

/// An open configuration interface of a real keyboard that may also be
/// written to (Phase 2). Opened with [`HidBackend::open_writable`].
///
/// Writes go through [`WriteTransport::write_command`], which only accepts a
/// [`WritePacket`]. Use it through a [`Writer`](crate::Writer).
pub struct WritableHidTransport(HidTransport);

impl WritableHidTransport {
    /// The descriptor this transport was opened from.
    pub fn descriptor(&self) -> &DeviceDescriptor {
        self.0.descriptor()
    }
}

impl Transport for WritableHidTransport {
    fn write(&mut self, packet: &OutPacket) -> Result<(), TransportError> {
        self.0.write(packet)
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Report>, TransportError> {
        self.0.read(timeout)
    }

    fn usb_info(&self) -> Option<UsbInfo> {
        self.0.usb_info()
    }
}

impl WriteTransport for WritableHidTransport {
    fn write_command(&mut self, packet: &WritePacket) -> Result<(), TransportError> {
        // hidapi expects the report ID first: 0x01 + 63 payload bytes.
        self.0
            .device
            .write(&packet.to_hid_report())
            .map(drop)
            .map_err(|e| TransportError::Write(e.to_string()))
    }
}

/// The error of a failed read. On Linux and macOS hidapi's `poll` fails with
/// `EINTR` when a signal handler runs (Ctrl+C: `poll` is never restarted,
/// whatever `SA_RESTART` says); its message is `strerror(EINTR)`. That is
/// [`TransportError::Interrupted`], which sessions retry, not a lost
/// connection.
fn read_error(message: &str) -> TransportError {
    if is_interrupted(message) {
        TransportError::Interrupted
    } else {
        TransportError::Read(message.to_string())
    }
}

/// Whether hidapi's message describes `EINTR`.
fn is_interrupted(message: &str) -> bool {
    message.contains("Interrupted system call") || message.contains("EINTR")
}

/// hidapi timeout in whole milliseconds, rounded up so that a short but
/// non-zero wait does not turn into a non-blocking poll.
fn timeout_millis(timeout: Duration) -> i32 {
    let millis = timeout.as_micros().div_ceil(1000);
    i32::try_from(millis).unwrap_or(i32::MAX)
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_vendor_collection_is_supported() {
        // The W75 configuration interface on the maintainer's machine.
        assert!(is_supported_interface(0x2E3C, 0xC368, 0xFF1B, 0x0091));
        // Other PIDs of the family.
        assert!(is_supported_interface(0x2E3C, 0xC365, 0xFF1B, 0x0091));
        // Keyboard and consumer-control collections of the same device.
        assert!(!is_supported_interface(0x2E3C, 0xC368, 0x0001, 0x0006));
        assert!(!is_supported_interface(0x2E3C, 0xC368, 0x000C, 0x0001));
        // Another vendor or an unknown PID.
        assert!(!is_supported_interface(0x1234, 0xC368, 0xFF1B, 0x0091));
        assert!(!is_supported_interface(0x2E3C, 0xC370, 0xFF1B, 0x0091));
    }

    #[test]
    fn timeouts_round_up() {
        assert_eq!(timeout_millis(Duration::ZERO), 0);
        assert_eq!(timeout_millis(Duration::from_micros(1)), 1);
        assert_eq!(timeout_millis(Duration::from_millis(100)), 100);
        assert_eq!(timeout_millis(Duration::from_secs(u64::MAX)), i32::MAX);
    }

    #[test]
    fn interrupted_reads_are_not_disconnects() {
        assert!(matches!(
            read_error("Interrupted system call"),
            TransportError::Interrupted
        ));
        assert!(matches!(
            read_error("hid_read_timeout: Error polling: EINTR"),
            TransportError::Interrupted
        ));
        assert!(matches!(
            read_error("hid_read_timeout: unexpected poll error (device disconnected)"),
            TransportError::Read(_)
        ));
    }

    #[test]
    fn empty_strings_become_none() {
        assert_eq!(non_empty(Some("  ")), None);
        assert_eq!(non_empty(Some("Wraith W75")), Some("Wraith W75".into()));
        assert_eq!(non_empty(None), None);
    }

    fn descriptor(path: &str, pid: u16) -> DeviceDescriptor {
        DeviceDescriptor {
            path: CString::new(path).expect("no NUL in test paths"),
            usb: UsbInfo {
                vendor_id: 0x2E3C,
                product_id: pid,
                product: Some("Wraith W75".into()),
                manufacturer: None,
                serial: None,
            },
            interface_number: 2,
            usage_page: 0xFF1B,
            usage: 0x0091,
        }
    }

    #[test]
    fn listings_carry_path_usb_and_interface() {
        let device = descriptor(r"\\?\hid#vid_2e3c&pid_c368&mi_02", 0xC368);
        let listing = device.listing();
        assert_eq!(listing.path, device.path());
        assert_eq!(listing.usb, device.usb);
        assert_eq!(listing.interface_number, 2);
        let json = serde_json::to_string(&listing).expect("serialisable");
        let back: DeviceListing = serde_json::from_str(&json).expect("deserialisable");
        assert_eq!(back, listing);
    }

    #[test]
    fn the_only_device_is_chosen_without_a_path() {
        let devices = [descriptor("a", 0xC368)];
        assert_eq!(choose_device(&devices, None), Ok(&devices[0]));
        assert_eq!(choose_device(&[], None), Err(DeviceChoiceError::NotFound));
        assert_eq!(DeviceChoiceError::NotFound.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn several_devices_need_a_path() {
        let devices = [descriptor("a", 0xC368), descriptor("b", 0xC365)];
        assert_eq!(
            choose_device(&devices, None),
            Err(DeviceChoiceError::Ambiguous { count: 2 })
        );
        assert_eq!(choose_device(&devices, Some("b")), Ok(&devices[1]));
        assert_eq!(
            choose_device(&devices, Some("c")),
            Err(DeviceChoiceError::UnknownPath { path: "c".into() })
        );
        assert_eq!(
            DeviceChoiceError::Ambiguous { count: 2 }.kind(),
            ErrorKind::InvalidRequest
        );
    }
}
