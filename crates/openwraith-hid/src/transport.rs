//! The [`Transport`] abstraction: something that can send an [`OutPacket`]
//! and receive input [`Report`]s, and its write extension
//! [`WriteTransport`].
//!
//! A plain [`Transport`] can only send [`OutPacket`]s, and the protocol crate
//! builds those only from read requests, so code that is generic over
//! `Transport` cannot write to the keyboard. Sending a
//! [`WritePacket`] needs the separate [`WriteTransport`] trait. There is
//! deliberately no method on `Transport` that could write: read-only code such
//! as [`read_all`](crate::read_all) is generic over `Transport` and therefore
//! provably cannot send a write.

use std::time::{Duration, Instant};

use openwraith_protocol::{OutPacket, Report, WritePacket};

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
    /// [`TransportError::Malformed`] for a report of the wrong size or ID
    /// (sessions skip those) and [`TransportError::Interrupted`] when a
    /// signal interrupted the wait (sessions read again).
    fn read(&mut self, timeout: Duration) -> Result<Option<Report>, TransportError>;

    /// USB identity of the device, if known.
    fn usb_info(&self) -> Option<UsbInfo> {
        None
    }

    /// The current time on this transport's clock. Sessions measure every
    /// timeout, observation period and expectation window with it.
    ///
    /// Real transports use the system clock (the default), and their
    /// [`read`](Self::read) blocks until a report arrives or the timeout
    /// passes. A simulator may keep a virtual clock instead that advances
    /// while `read` "waits", so that timing rules can be tested without
    /// sleeping. Either way the clock must advance while `read` waits:
    /// waits such as the writer's observation period end only when this
    /// clock says so.
    fn now(&self) -> Instant {
        Instant::now()
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

    fn now(&self) -> Instant {
        (**self).now()
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

    fn now(&self) -> Instant {
        (**self).now()
    }
}

/// A transport that can also send write packets (Phase 2).
///
/// Implemented by [`WritableHidTransport`](crate::WritableHidTransport)
/// (opened with [`HidBackend::open_writable`](crate::HidBackend::open_writable))
/// and, with the `mock` feature, by the device simulator `DeviceSim`.
/// [`HidTransport`](crate::HidTransport) and `MockTransport` do **not**
/// implement it, so a [`Session`](crate::Session) over them has no
/// [`writer`](crate::Session::writer).
///
/// A [`WritePacket`] can only be built by
/// [`WriteRequest::steps`](openwraith_protocol::WriteRequest::steps), which
/// produces the nine write forms confirmed on hardware. Use a
/// [`Writer`](crate::Writer) instead of calling this directly: it waits for
/// every acknowledgement, verifies each write by reading it back and never
/// answers a keyboard reset.
pub trait WriteTransport: Transport {
    /// Sends one write packet.
    fn write_command(&mut self, packet: &WritePacket) -> Result<(), TransportError>;
}

impl<T: WriteTransport + ?Sized> WriteTransport for &mut T {
    fn write_command(&mut self, packet: &WritePacket) -> Result<(), TransportError> {
        (**self).write_command(packet)
    }
}

impl<T: WriteTransport + ?Sized> WriteTransport for Box<T> {
    fn write_command(&mut self, packet: &WritePacket) -> Result<(), TransportError> {
        (**self).write_command(packet)
    }
}
