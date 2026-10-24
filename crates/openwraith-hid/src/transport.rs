//! The [`Transport`] abstraction: something that can send an [`OutPacket`]
//! and receive input [`Report`]s.
//!
//! Only [`OutPacket`]s can be sent, and the protocol crate builds those only
//! from read requests, so no transport can be used to write to the keyboard.

use std::time::Duration;

use openwraith_protocol::{OutPacket, Report};

use crate::error::TransportError;

/// USB identity of an opened device, as far as the transport knows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsbInfo {
    /// USB vendor ID.
    pub vendor_id: u16,
    /// USB product ID.
    pub product_id: u16,
    /// USB product string (for example `"Wraith W75"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// USB manufacturer string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// USB serial number string, if the device has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
}

/// A connection to the keyboard's configuration interface.
///
/// Implementations: [`HidTransport`](crate::HidTransport) for a real keyboard
/// and, with the `mock` feature, `MockTransport` for captured traffic.
pub trait Transport {
    /// Sends one output report.
    fn write(&mut self, packet: &OutPacket) -> Result<(), TransportError>;

    /// Waits up to `timeout` for the next input report.
    ///
    /// Returns `Ok(None)` when nothing arrived within `timeout`. A zero
    /// timeout polls without blocking. Implementations return
    /// [`TransportError::Malformed`] for a report of the wrong size or ID;
    /// sessions skip those.
    fn read(&mut self, timeout: Duration) -> Result<Option<Report>, TransportError>;

    /// USB identity of the device, if known.
    fn usb_info(&self) -> Option<UsbInfo> {
        None
    }
}

impl<T: Transport + ?Sized> Transport for &mut T {
    fn write(&mut self, packet: &OutPacket) -> Result<(), TransportError> {
        (**self).write(packet)
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Report>, TransportError> {
        (**self).read(timeout)
    }

    fn usb_info(&self) -> Option<UsbInfo> {
        (**self).usb_info()
    }
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn write(&mut self, packet: &OutPacket) -> Result<(), TransportError> {
        (**self).write(packet)
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Report>, TransportError> {
        (**self).read(timeout)
    }

    fn usb_info(&self) -> Option<UsbInfo> {
        (**self).usb_info()
    }
}
