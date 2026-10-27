//! Real keyboards through hidapi. Not part of simulator builds.
//!
//! hidapi allows one `HidApi` per process; the worker creates this backend
//! inside its own thread and it never leaves it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use openwraith_hid::{DeviceDescriptor, HidBackend, TransportError, WritableHidTransport};

use super::{DeviceBackend, DeviceEntry};
use crate::clock::{Clock, SystemClock};
use crate::tokens::TokenMint;

/// The hidapi backend.
pub(crate) struct HidDeviceBackend {
    hid: HidBackend,
    ids: TokenMint,
    found: HashMap<String, DeviceDescriptor>,
}

impl HidDeviceBackend {
    /// Initialises hidapi (opens nothing).
    pub(crate) fn new(seed: Option<u64>) -> Result<Self, TransportError> {
        Ok(HidDeviceBackend {
            hid: HidBackend::new()?,
            ids: TokenMint::new(seed),
            found: HashMap::new(),
        })
    }
}

impl DeviceBackend for HidDeviceBackend {
    type Transport = WritableHidTransport;

    fn enumerate(&mut self) -> Result<Vec<DeviceEntry>, TransportError> {
        self.hid.refresh()?;
        let devices = self.hid.devices();
        self.found.clear();
        let mut entries = Vec::with_capacity(devices.len());
        for device in devices {
            let listing = device.listing();
            let id = self.ids.device(&listing.path);
            self.found.insert(id.0.clone(), device);
            entries.push(DeviceEntry { id, listing });
        }
        Ok(entries)
    }

    fn open(&mut self, entry: &DeviceEntry) -> Result<WritableHidTransport, TransportError> {
        let device = self
            .found
            .get(&entry.id.0)
            .ok_or_else(|| TransportError::Open {
                path: entry.listing.path.clone(),
                message: "the keyboard is no longer listed".to_string(),
            })?;
        self.hid.open_writable(device)
    }

    fn is_simulated(&self) -> bool {
        false
    }

    fn clock(&self) -> Arc<dyn Clock> {
        Arc::new(SystemClock)
    }

    fn idle(&mut self, _duration: Duration) {}

    fn idle_waits(&self) -> bool {
        false
    }
}
