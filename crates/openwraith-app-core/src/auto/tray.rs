//! The tray icon's model and the rules of a tray click (design B5). Pure:
//! made from the published status, the local profile names and what the
//! frontend reported.

use crate::dto::{ConnectionState, DeviceStatus, UiStateReport};
use crate::shell::{
    OpenReason, TrayAttention, TrayHeader, TrayModel, TrayOutcome, TrayProfileItem, TrayState,
};

use super::notify::{ROUTE_DEPTH, ROUTE_DEVICE, ROUTE_GAMES, ROUTE_RECOVERY};

/// Whether the status shows a connected keyboard (opened, whatever it does).
fn connected(status: &DeviceStatus) -> bool {
    !matches!(
        status.connection,
        ConnectionState::Starting
            | ConnectionState::Searching
            | ConnectionState::SeveralKeyboards { .. }
            | ConnectionState::Opening { .. }
            | ConnectionState::OpenFailed { .. }
            | ConnectionState::Lost { .. }
            | ConnectionState::Unsupported { .. }
            | ConnectionState::Released
    )
}

/// The attention items of the status.
fn attention(status: &DeviceStatus) -> Vec<TrayAttention> {
    let mut items = Vec::new();
    if !status.recovery.reset_suspected.is_empty()
        || status.recovery.reset_reported
        || matches!(status.connection, ConnectionState::ResetSeen)
    {
        items.push(TrayAttention::ResetReview);
    }
    if matches!(status.connection, ConnectionState::OtherProgram { .. }) {
        items.push(TrayAttention::OtherProgramCheck);
    }
    if status.travel.marker_left
        || status.travel.stray_reports > 0
        || matches!(status.travel.state, crate::dto::TravelStateView::MaybeOpen)
    {
        items.push(TrayAttention::TravelStop);
    }
    if status
        .automation
        .paused
        .as_ref()
        .is_some_and(|r| !matches!(r, crate::dto::AutoPauseReason::User))
    {
        items.push(TrayAttention::AutoPaused);
    }
    items
}

/// The tray model of `status` with the local names of the keyboard's
/// profiles (four entries, `None` = no name); "Back up now" only while
/// automatic backups are on (`auto_backup`).
pub(crate) fn model(
    status: &DeviceStatus,
    names: &[Option<String>],
    auto_backup: bool,
) -> TrayModel {
    let name = |p: u8| {
        names
            .get(usize::from(p).saturating_sub(1))
            .cloned()
            .flatten()
    };
    let is_connected = connected(status);
    let ready = matches!(status.connection, ConnectionState::Ready);
    let header = if is_connected {
        TrayHeader::Connected {
            model: status
                .device
                .as_ref()
                .and_then(|d| d.model.clone().or_else(|| d.product.clone())),
            profile: status.active_profile,
            name: status.active_profile.and_then(name),
        }
    } else {
        TrayHeader::NotConnected
    };
    let attention = attention(status);
    let state = if !attention.is_empty() {
        TrayState::Attention
    } else if is_connected {
        TrayState::Normal
    } else {
        TrayState::Disconnected
    };
    TrayModel {
        state,
        header,
        profiles: (1..=4)
            .map(|p| TrayProfileItem {
                profile: p,
                name: name(p),
                checked: status.active_profile == Some(p),
                enabled: ready,
            })
            .collect(),
        auto_paused: status
            .automation
            .enabled
            .then_some(status.automation.paused.is_some()),
        back_up_now: auto_backup && ready && status.busy.is_none() && !status.paused_for_cli,
        attention,
    }
}

/// What a tray profile click does: `None` = switch through the manual
/// profile switch; else the window opens (drafts exist, a plan is pending,
/// writes are blocked, or this run has no vendor-site acknowledgement yet)
/// or nothing happens (already active).
pub(crate) fn profile_click(
    profile: u8,
    status: &DeviceStatus,
    ui: &UiStateReport,
    plan_pending: bool,
) -> Option<TrayOutcome> {
    let open = |reason| {
        Some(TrayOutcome::OpenWindow {
            route: ROUTE_DEVICE.to_string(),
            reason,
        })
    };
    if !(1..=4).contains(&profile) {
        return Some(TrayOutcome::NothingToDo);
    }
    if ui.drafts {
        return open(OpenReason::Drafts);
    }
    if plan_pending {
        return open(OpenReason::PlanPending);
    }
    if !status.write_gate.allowed || status.paused_for_cli {
        return open(OpenReason::WritesBlocked);
    }
    if !ui.vendor_acknowledged {
        return open(OpenReason::NoVendorAcknowledgement);
    }
    if status.active_profile == Some(profile) {
        return Some(TrayOutcome::NothingToDo);
    }
    None
}

/// Where an attention item leads (`None`: it acts without the window).
pub(crate) fn attention_route(item: TrayAttention) -> Option<&'static str> {
    match item {
        TrayAttention::ResetReview => Some(ROUTE_RECOVERY),
        TrayAttention::OtherProgramCheck => None,
        TrayAttention::TravelStop => Some(ROUTE_DEPTH),
        TrayAttention::AutoPaused => Some(ROUTE_GAMES),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{
        AutoPauseReason, AutomationStatusView, RecoveryStatusView, TravelStateView,
        TravelStatusView, WriteBlockReason, WriteGate,
    };

    fn status(connection: ConnectionState, allowed: bool) -> DeviceStatus {
        DeviceStatus {
            revision: 1,
            generation: 1,
            connection,
            device: None,
            active_profile: Some(1),
            firmware_tested: None,
            write_gate: WriteGate {
                allowed,
                reasons: if allowed {
                    Vec::new()
                } else {
                    vec![WriteBlockReason::OtherProgram]
                },
            },
            busy: None,
            snapshot_revision: Some(1),
            snapshot_stale: false,
            close_requested: false,
            interrupted: None,
            sim: false,
            travel: TravelStatusView {
                state: TravelStateView::Closed,
                stray_reports: 0,
                marker_left: false,
            },
            recovery: RecoveryStatusView::default(),
            automation: AutomationStatusView::default(),
            paused_for_cli: false,
        }
    }

    const ACK: UiStateReport = UiStateReport {
        drafts: false,
        vendor_acknowledged: true,
    };

    #[test]
    fn a_profile_click_switches_only_when_nothing_stands_in_the_way() {
        let ready = status(ConnectionState::Ready, true);
        assert_eq!(profile_click(2, &ready, &ACK, false), None, "switch");
        assert_eq!(
            profile_click(1, &ready, &ACK, false),
            Some(TrayOutcome::NothingToDo)
        );
        let open = |o: Option<TrayOutcome>| match o {
            Some(TrayOutcome::OpenWindow { reason, .. }) => Some(reason),
            _ => None,
        };
        let drafts = UiStateReport {
            drafts: true,
            ..ACK
        };
        assert_eq!(
            open(profile_click(2, &ready, &drafts, false)),
            Some(OpenReason::Drafts)
        );
        assert_eq!(
            open(profile_click(2, &ready, &ACK, true)),
            Some(OpenReason::PlanPending)
        );
        let other = status(
            ConnectionState::OtherProgram {
                evidence: crate::dto::ForeignActivityView {
                    status_reports: 1,
                    foreign_acks: 0,
                    profile_reports: 0,
                },
            },
            false,
        );
        assert_eq!(
            open(profile_click(2, &other, &ACK, false)),
            Some(OpenReason::WritesBlocked)
        );
        assert_eq!(
            open(profile_click(2, &ready, &UiStateReport::default(), false)),
            Some(OpenReason::NoVendorAcknowledgement)
        );
        let mut cli = ready.clone();
        cli.paused_for_cli = true;
        assert_eq!(
            open(profile_click(2, &cli, &ACK, false)),
            Some(OpenReason::WritesBlocked)
        );
    }

    #[test]
    fn the_model_follows_the_status() {
        let names = [None, Some("FPS".to_string()), None, None];
        let mut ready = status(ConnectionState::Ready, true);
        ready.active_profile = Some(2);
        let m = model(&ready, &names, true);
        assert_eq!(m.state, TrayState::Normal);
        assert_eq!(
            m.header,
            TrayHeader::Connected {
                model: None,
                profile: Some(2),
                name: Some("FPS".to_string())
            }
        );
        assert!(m.profiles[1].checked && m.profiles.iter().all(|p| p.enabled));
        assert_eq!(m.auto_paused, None, "automation off: no item");
        assert!(m.back_up_now);

        let m = model(&status(ConnectionState::Searching, false), &names, true);
        assert_eq!(
            (m.state, m.header),
            (TrayState::Disconnected, TrayHeader::NotConnected)
        );
        assert!(!m.back_up_now && m.profiles.iter().all(|p| !p.enabled));

        let mut reset = status(ConnectionState::ResetSeen, false);
        reset.recovery.reset_reported = true;
        let m = model(&reset, &names, true);
        assert_eq!(
            (m.state, m.attention.clone()),
            (TrayState::Attention, vec![TrayAttention::ResetReview])
        );

        let mut paused = ready.clone();
        paused.automation = AutomationStatusView {
            enabled: true,
            paused: Some(AutoPauseReason::RateLimit),
            holding: None,
            waiting: None,
        };
        paused.travel.marker_left = true;
        let m = model(&paused, &names, true);
        assert_eq!(m.auto_paused, Some(true));
        assert_eq!(
            m.attention,
            vec![TrayAttention::TravelStop, TrayAttention::AutoPaused]
        );
        assert_eq!(attention_route(TrayAttention::OtherProgramCheck), None);
    }
}
