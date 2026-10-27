//! The published status.

use super::{Conn, Worker};
use crate::backend::DeviceBackend;
use crate::config::TESTED_FIRMWARE;
use crate::dto::{DeviceStatus, WriteBlockReason, WriteGate};

impl<B: DeviceBackend> Worker<B> {
    /// Why settings may not be written now (empty: they may).
    pub(crate) fn write_block_reasons(&self) -> Vec<WriteBlockReason> {
        let mut reasons = Vec::new();
        match &self.conn {
            Conn::Ready => {}
            Conn::Observing | Conn::Reading { .. } => reasons.push(WriteBlockReason::Observing),
            Conn::OtherProgram { .. } => reasons.push(WriteBlockReason::OtherProgram),
            Conn::ResetSeen => reasons.push(WriteBlockReason::ResetSeen),
            Conn::Fault { .. } => reasons.push(WriteBlockReason::Fault),
            Conn::Starting
            | Conn::Searching
            | Conn::Several
            | Conn::OpenFailed { .. }
            | Conn::ReadFailed { .. }
            | Conn::Lost { .. }
            | Conn::Unsupported { .. }
            | Conn::Released => reasons.push(WriteBlockReason::NotConnected),
        }
        if self.current.is_some() {
            reasons.push(WriteBlockReason::Busy);
        }
        if self.stale {
            reasons.push(WriteBlockReason::SnapshotStale);
        }
        if self.config.safety_dir.is_err() {
            reasons.push(WriteBlockReason::SafetyDirUnavailable);
        }
        reasons
    }

    /// The status as the worker sees it.
    pub(crate) fn status(&self) -> DeviceStatus {
        let now = self.now();
        let device = self.snapshot.as_ref().map(|s| s.device.clone());
        let firmware_tested = device
            .as_ref()
            .and_then(|d| d.firmware.as_deref())
            .map(|f| TESTED_FIRMWARE.contains(&f));
        let reasons = self.write_block_reasons();
        DeviceStatus {
            revision: 0,
            generation: self.generation,
            connection: self.connection_state(now),
            device,
            active_profile: self.active_profile,
            firmware_tested,
            write_gate: WriteGate {
                allowed: reasons.is_empty(),
                reasons,
            },
            busy: self.current.clone(),
            snapshot_revision: self.snapshot.as_ref().map(|s| s.revision),
            snapshot_stale: self.stale,
            close_requested: false,
            interrupted: None,
            sim: self.sim,
        }
    }

    /// Publishes the status (when it changed).
    pub(crate) fn publish(&self) {
        self.shared.publish(self.status(), self.now());
    }
}
