//! Where keyboards come from: hidapi, or the developer simulator.

#[cfg(not(feature = "sim"))]
pub(crate) mod hid;
#[cfg(any(test, feature = "sim"))]
pub(crate) mod sim;

use std::sync::Arc;
use std::time::Duration;

use openwraith_hid::{DeviceListing, TransportError, WriteTransport};

use crate::clock::Clock;
use crate::dto::{DeviceEntryView, DeviceId};

/// A connected keyboard found by enumeration. The path never leaves Rust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceEntry {
    /// Its id (a keyed hash of the path).
    pub(crate) id: DeviceId,
    /// Its description.
    pub(crate) listing: DeviceListing,
}

impl DeviceEntry {
    /// The entry for the frontend.
    pub(crate) fn view(&self) -> DeviceEntryView {
        DeviceEntryView {
            id: self.id.clone(),
            vendor_id: self.listing.usb.vendor_id,
            product_id: self.listing.usb.product_id,
            product: self.listing.usb.product.as_deref().map(crate::map::clean),
            serial: self.listing.usb.serial.as_deref().map(crate::map::clean),
            interface: self.listing.interface_number,
        }
    }
}

/// A source of keyboards.
pub(crate) trait DeviceBackend {
    /// The transport of an opened keyboard.
    type Transport: WriteTransport;

    /// Lists the connected keyboards (opens nothing).
    fn enumerate(&mut self) -> Result<Vec<DeviceEntry>, TransportError>;

    /// Opens a keyboard for reading and writing.
    fn open(&mut self, entry: &DeviceEntry) -> Result<Self::Transport, TransportError>;

    /// Whether this is the developer simulator.
    fn is_simulated(&self) -> bool;

    /// The clock the worker uses while no keyboard is open (the same one
    /// the transports use).
    fn clock(&self) -> Arc<dyn Clock>;

    /// Lets `duration` pass while no keyboard is open (the simulator's
    /// virtual time; nothing for hidapi, where the worker loop waits).
    fn idle(&mut self, duration: Duration);

    /// Whether [`idle`](Self::idle) itself takes the time (then the worker
    /// loop does not wait as well).
    fn idle_waits(&self) -> bool;
}
