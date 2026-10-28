//! The published status.

use super::{Conn, Worker};
use crate::backend::DeviceBackend;
use crate::config::TESTED_FIRMWARE;
use crate::dto::{DeviceStatus, TravelStateView, TravelStatusView, WriteBlockReason, WriteGate};

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
        if self.paused_for_cli() {
            reasons.push(WriteBlockReason::PausedForCli);
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
            travel: self.travel_status(),
            recovery: self.shared.recovery_status(),
            automation: self.shared.automation_status(),
            paused_for_cli: self.paused_for_cli(),
        }
    }

    /// Whether a key-depth test is (or may be) open on the keyboard.
    pub(crate) fn travel_status(&self) -> TravelStatusView {
        let session = self.session.as_ref();
        TravelStatusView {
            state: match session.map(openwraith_hid::Session::travel_state) {
                Some(openwraith_hid::TravelState::Open) => TravelStateView::Open,
                Some(openwraith_hid::TravelState::MaybeOpen) => TravelStateView::MaybeOpen,
                Some(openwraith_hid::TravelState::Closed) | None => TravelStateView::Closed,
            },
            stray_reports: session.map_or(0, |s| crate::map::sat32(s.stray_travel_since_gate())),
            marker_left: self.shared.travel_marker_left(),
        }
    }

    /// Publishes the status (when it changed).
    pub(crate) fn publish(&self) {
        self.shared
            .plan_pending
            .store(self.pending.is_some(), std::sync::atomic::Ordering::SeqCst);
        self.shared.publish(self.status(), self.now());
    }
}
