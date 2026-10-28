//! OS notifications (design B5): which notices become one, with which text
//! key, parameters and route, after the user's settings (`notifications.reset`
//! for resets, firmware changes and failed automatic backups;
//! `notifications.auto_switch` for game switches, default off) and at most
//! one per kind every 10 minutes. Pure: the caller passes the time.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::dto::{AutoCauseView, AutoPauseReason, AutoWaitReason, Notice, NotificationSettings};
use crate::shell::{NotificationKind, OsNotification};

/// At most one notification of a kind this often.
pub(crate) const THROTTLE: Duration = Duration::from_secs(600);

/// The route of the Recovery card.
pub(crate) const ROUTE_RECOVERY: &str = "#/backup?recover=1";
/// The route of Settings > Games.
pub(crate) const ROUTE_GAMES: &str = "#/settings?tab=games";
/// The route of the keyboard page.
pub(crate) const ROUTE_DEVICE: &str = "#/device";
/// The route of the automatic backups (the page brings the card into view).
pub(crate) const ROUTE_BACKUPS: &str = "#/backup?section=auto";
/// The route of the key-depth tab.
pub(crate) const ROUTE_DEPTH: &str = "#/actuation?tab=depth";

/// The throttle (one per kind per 10 minutes).
#[derive(Debug, Default)]
pub(crate) struct Notifier {
    last: BTreeMap<NotificationKind, Instant>,
}

fn params(pairs: &[(&str, String)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

fn notification(
    kind: NotificationKind,
    key: &str,
    params: BTreeMap<String, String>,
    route: &str,
) -> OsNotification {
    OsNotification {
        kind,
        key: key.to_string(),
        params,
        route: route.to_string(),
    }
}

/// The game's name of an automatic switch (`label`), or the profile only.
fn switch_params(label: Option<&String>, profile: u8) -> BTreeMap<String, String> {
    let mut p = params(&[("profile", profile.to_string())]);
    if let Some(label) = label {
        p.insert("label".to_string(), label.clone());
    }
    p
}

/// The OS notification a notice deserves under `settings`, if any (not
/// throttled yet).
pub(crate) fn for_notice(
    notice: &Notice,
    settings: &NotificationSettings,
) -> Option<OsNotification> {
    match notice {
        Notice::ResetSuspected { profile, .. } if settings.reset => Some(notification(
            NotificationKind::ResetSuspected,
            "notifications.resetSuspected",
            params(&[("profile", profile.to_string())]),
            ROUTE_RECOVERY,
        )),
        Notice::FirmwareChanged { known, current } if settings.reset => Some(notification(
            NotificationKind::FirmwareChanged,
            "notifications.firmwareChanged",
            params(&[
                ("known", known.version.clone().unwrap_or_default()),
                ("current", current.version.clone().unwrap_or_default()),
            ]),
            ROUTE_RECOVERY,
        )),
        Notice::AutoBackupFailed { profile, .. } if settings.reset => Some(notification(
            NotificationKind::AutoBackupFailed,
            "notifications.autoBackupFailed",
            params(&[("profile", profile.to_string())]),
            ROUTE_BACKUPS,
        )),
        Notice::AutoSwitched {
            label, to, cause, ..
        } if settings.auto_switch => Some(notification(
            NotificationKind::AutoSwitched,
            match cause {
                AutoCauseView::GameInFront => "notifications.autoSwitched",
                AutoCauseView::GameExited => "notifications.autoSwitchedBack",
            },
            switch_params(label.as_ref(), *to),
            ROUTE_GAMES,
        )),
        Notice::AutoSwitchDeferred {
            label,
            profile,
            reason: AutoWaitReason::Drafts,
            ..
        } => Some(notification(
            NotificationKind::AutoSwitchWaiting,
            "notifications.autoSwitchWaiting",
            switch_params(label.as_ref(), *profile),
            ROUTE_GAMES,
        )),
        // B5: a switch skipped because of another program (or the CLI),
        // and one whose wait for drafts was announced; every other reason
        // keeps the in-app record only.
        Notice::AutoSwitchSkipped {
            label,
            profile,
            reason:
                reason @ (AutoWaitReason::OtherProgram
                | AutoWaitReason::RecentForeignActivity
                | AutoWaitReason::PausedForCli
                | AutoWaitReason::Drafts),
            ..
        } => {
            let mut p = switch_params(label.as_ref(), *profile);
            p.insert("reason".to_string(), wait_reason_key(*reason).to_string());
            Some(notification(
                NotificationKind::AutoSwitchSkipped,
                "notifications.autoSwitchSkipped",
                p,
                ROUTE_GAMES,
            ))
        }
        Notice::AutoProfilePaused { reason } => match reason {
            AutoPauseReason::User => None,
            AutoPauseReason::RateLimit => Some(notification(
                NotificationKind::AutoPaused,
                "notifications.autoPausedRateLimit",
                BTreeMap::new(),
                ROUTE_GAMES,
            )),
            AutoPauseReason::SwitchFailed { .. } => Some(notification(
                NotificationKind::AutoPaused,
                "notifications.autoPausedFailed",
                BTreeMap::new(),
                ROUTE_GAMES,
            )),
        },
        _ => None,
    }
}

/// The notification of the keyboard's reset report (R1), sent when the
/// worker enters `ResetSeen`.
pub(crate) fn keyboard_reset(settings: &NotificationSettings) -> Option<OsNotification> {
    settings.reset.then(|| {
        notification(
            NotificationKind::KeyboardReset,
            "notifications.keyboardReset",
            BTreeMap::new(),
            ROUTE_DEVICE,
        )
    })
}

/// A wait reason as the key the glue appends to `notifications.reasons.`.
pub(crate) fn wait_reason_key(reason: AutoWaitReason) -> &'static str {
    match reason {
        AutoWaitReason::NotConnected => "not_connected",
        AutoWaitReason::Connecting => "connecting",
        AutoWaitReason::Busy => "busy",
        AutoWaitReason::PlanPending => "plan_pending",
        AutoWaitReason::Drafts => "drafts",
        AutoWaitReason::DepthView => "depth_view",
        AutoWaitReason::TravelTest => "travel_test",
        AutoWaitReason::OtherProgram => "other_program",
        AutoWaitReason::PausedForCli => "paused_for_cli",
        AutoWaitReason::ResetSeen => "reset_seen",
        AutoWaitReason::Released => "released",
        AutoWaitReason::Fault => "fault",
        AutoWaitReason::RecentForeignActivity => "recent_foreign_activity",
        AutoWaitReason::KeymapUnsaved => "keymap_unsaved",
        AutoWaitReason::ProfileChanging => "profile_changing",
    }
}

impl Notifier {
    /// Whether `notification` may be shown now (one per kind per
    /// [`THROTTLE`]); remembers it when it may.
    pub(crate) fn admit(&mut self, notification: &OsNotification, now: Instant) -> bool {
        match self.last.get(&notification.kind) {
            Some(at) if now.saturating_duration_since(*at) < THROTTLE => false,
            _ => {
                self.last.insert(notification.kind, now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switched(label: &str) -> Notice {
        Notice::AutoSwitched {
            rule_id: Some("r-1".to_string()),
            label: Some(label.to_string()),
            from: Some(1),
            to: 2,
            cause: AutoCauseView::GameInFront,
        }
    }

    #[test]
    fn one_notification_per_kind_per_ten_minutes() {
        let t0 = Instant::now();
        let mut n = Notifier::default();
        let on = NotificationSettings {
            reset: true,
            auto_switch: true,
        };
        let a = for_notice(&switched("A"), &on).unwrap();
        assert!(n.admit(&a, t0));
        assert!(!n.admit(&a, t0 + Duration::from_secs(599)), "throttled");
        let reset = for_notice(
            &Notice::ResetSuspected {
                profile: 1,
                evidence: Vec::new(),
            },
            &on,
        )
        .unwrap();
        assert!(n.admit(&reset, t0 + Duration::from_secs(1)), "another kind");
        assert!(n.admit(&a, t0 + Duration::from_secs(600)));
        assert_eq!(a.route, ROUTE_GAMES);
        assert_eq!(a.params["label"], "A");
        assert_eq!(reset.route, ROUTE_RECOVERY);
    }

    #[test]
    fn every_key_sent_is_in_the_list_the_glue_words_and_back() {
        use std::collections::BTreeSet;
        let all = NotificationSettings {
            reset: true,
            auto_switch: true,
        };
        let mut notices = vec![
            Notice::ResetSuspected {
                profile: 1,
                evidence: Vec::new(),
            },
            Notice::FirmwareChanged {
                known: crate::dto::FirmwareView {
                    version: None,
                    build_date: None,
                },
                current: crate::dto::FirmwareView {
                    version: None,
                    build_date: None,
                },
            },
            Notice::AutoBackupFailed {
                profile: 1,
                message: String::new(),
            },
        ];
        for cause in [AutoCauseView::GameInFront, AutoCauseView::GameExited] {
            notices.push(Notice::AutoSwitched {
                rule_id: None,
                label: None,
                from: None,
                to: 1,
                cause,
            });
            for reason in [
                AutoWaitReason::Drafts,
                AutoWaitReason::OtherProgram,
                AutoWaitReason::Busy,
            ] {
                notices.push(Notice::AutoSwitchDeferred {
                    rule_id: None,
                    label: None,
                    profile: 1,
                    cause,
                    reason,
                });
                notices.push(Notice::AutoSwitchSkipped {
                    rule_id: None,
                    label: None,
                    profile: 1,
                    cause,
                    reason,
                });
            }
        }
        for reason in [
            AutoPauseReason::User,
            AutoPauseReason::RateLimit,
            AutoPauseReason::SwitchFailed {
                message: String::new(),
            },
        ] {
            notices.push(Notice::AutoProfilePaused { reason });
        }
        let mut sent: BTreeSet<String> = notices
            .iter()
            .filter_map(|n| for_notice(n, &all))
            .map(|n| n.key)
            .collect();
        sent.extend(keyboard_reset(&all).map(|n| n.key));
        let listed: BTreeSet<String> = crate::shell::NOTIFICATION_KEYS
            .iter()
            .map(|k| (*k).to_string())
            .collect();
        assert_eq!(sent, listed);
    }

    #[test]
    fn the_settings_choose_the_notifications() {
        let default = NotificationSettings::default();
        assert!(
            for_notice(&switched("A"), &default).is_none(),
            "game switches are opt-in"
        );
        let off = NotificationSettings {
            reset: false,
            auto_switch: false,
        };
        assert!(
            for_notice(
                &Notice::ResetSuspected {
                    profile: 1,
                    evidence: Vec::new()
                },
                &off
            )
            .is_none()
        );
        assert!(keyboard_reset(&off).is_none());
        assert!(keyboard_reset(&default).is_some());
        // Waiting (drafts) and skipped switches are always told; a user
        // pause never is.
        let waiting = Notice::AutoSwitchDeferred {
            rule_id: None,
            label: None,
            profile: 1,
            cause: AutoCauseView::GameExited,
            reason: AutoWaitReason::Drafts,
        };
        assert_eq!(
            for_notice(&waiting, &off).unwrap().kind,
            NotificationKind::AutoSwitchWaiting
        );
        let busy = Notice::AutoSwitchDeferred {
            rule_id: None,
            label: None,
            profile: 1,
            cause: AutoCauseView::GameExited,
            reason: AutoWaitReason::Busy,
        };
        assert!(for_notice(&busy, &off).is_none());
        let skipped = Notice::AutoSwitchSkipped {
            rule_id: None,
            label: None,
            profile: 1,
            cause: AutoCauseView::GameExited,
            reason: AutoWaitReason::OtherProgram,
        };
        assert_eq!(
            for_notice(&skipped, &off).unwrap().params["reason"],
            "other_program"
        );
        assert!(
            for_notice(
                &Notice::AutoProfilePaused {
                    reason: AutoPauseReason::User
                },
                &off
            )
            .is_none()
        );
        assert!(
            for_notice(
                &Notice::AutoProfilePaused {
                    reason: AutoPauseReason::RateLimit
                },
                &off
            )
            .is_some()
        );
    }
}
