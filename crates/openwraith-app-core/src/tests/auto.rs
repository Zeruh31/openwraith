//! The per-game automatic profiles on the worker (virtual clock): the
//! dwell, the exit, manual overrides, every deferral, the commit settle, a
//! failed switch, the 180 s rule of the tray mode, the tray's click rules
//! and the notification throttle. The automatic path sends nothing but the
//! profile select and reads (B15 invariant, checked on the simulator's
//! packets).

use std::sync::Arc;
use std::time::Duration;

use super::harness::{Harness, pos};
use crate::dto::{
    AppErrorKind, AutoCauseView, AutoEventView, AutoPauseReason, AutoProfileRuleIntent,
    AutoProfileRulesIntent, AutoWaitReason, BackgroundSettingsIntent, Notice, RtIntent,
    RuleProgramIntent, SimAction, UiStateReport, WriteBlockReason,
};
use crate::handle::WorkerHandle;
use crate::shell::{
    ForegroundProgram, NotificationKind, OpenReason, OsNotification, RunningProgram, TrayAction,
    TrayAttention, TrayModel, TrayOutcome,
};
use crate::sink::recorder::{FixedPrograms, Recorder};
use crate::worker::{Conn, Request};

/// The handle of a harness for direct commands (never an operation: no
/// thread answers the harness's requests).
fn handle(h: &Harness) -> WorkerHandle {
    WorkerHandle::from_parts(
        h.tx.clone(),
        Arc::clone(&h.shared),
        h.worker.config.clone(),
        None,
        Some(h.sim.clone()),
    )
}

/// The rules `(exe, profile)`, switched on.
fn rules(h: &Harness, rules: &[(&str, u8)], default: Option<u8>) {
    let intent = AutoProfileRulesIntent {
        enabled: true,
        default_profile: default,
        rules: rules
            .iter()
            .map(|(exe, profile)| AutoProfileRuleIntent {
                id: None,
                label: exe.trim_end_matches(".exe").to_string(),
                program: RuleProgramIntent::Name {
                    exe: (*exe).to_string(),
                },
                profile: *profile,
                enabled: true,
            })
            .collect(),
    };
    handle(h).set_auto_profile_rules(&intent).unwrap();
}

/// `exe` (process `pid`) comes to the front.
fn front(h: &Harness, pid: u32, exe: &str) {
    handle(h).foreground_changed(Some(ForegroundProgram {
        pid,
        exe: Some(exe.to_string()),
        path: Some(format!(r"C:\Games\{exe}")),
        elevated: false,
    }));
}

fn auto_notices(h: &Harness) -> Vec<Notice> {
    h.notices()
        .into_iter()
        .filter(|n| {
            matches!(
                n,
                Notice::AutoSwitched { .. }
                    | Notice::AutoSwitchDeferred { .. }
                    | Notice::AutoSwitchSkipped { .. }
                    | Notice::AutoProfilePaused { .. }
            )
        })
        .collect()
}

fn switched_to(h: &Harness) -> Vec<(u8, AutoCauseView)> {
    auto_notices(h)
        .into_iter()
        .filter_map(|n| match n {
            Notice::AutoSwitched { to, cause, .. } => Some((to, cause)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_game_in_front_switches_its_profile_with_only_the_select_and_reads() {
    let mut h = Harness::ready();
    rules(&h, &[("game.exe", 2)], Some(1));
    let start = h.packets();
    let writes = h.writes();
    front(&h, 100, "game.exe");
    h.ticks(13);
    assert_eq!(h.writes(), writes, "not before 1.5 s in front");
    assert!(h.until(|w| w.active_profile == Some(2), 40));
    // The automatic path: one profile select, then reads only.
    let sent = h.packets_since(start);
    let written: Vec<_> = sent.iter().filter(|p| p.is_write()).collect();
    assert_eq!(written.len(), 1, "only the select");
    assert_eq!(written[0].as_bytes()[..7], [0x21, 0, 0, 0, 0, 0x12, 2]);
    assert!(sent.len() > 10, "the new profile was read");
    assert_eq!(h.snapshot().profile, 2);
    let status = h.status();
    let hold = status.automation.holding.expect("the Auto chip");
    assert_eq!((hold.rule_id.as_str(), hold.profile), ("r-1", 2));
    assert_eq!(switched_to(&h), [(2, AutoCauseView::GameInFront)]);
    // The automation log names the rule.
    let log = std::fs::read_to_string(h.worker.config.state_dir.join("automation.log")).unwrap();
    assert!(log.contains("\"rule_id\":\"r-1\""), "{log}");
    let state = handle(&h).auto_profile_state();
    assert!(matches!(
        state.recent.first().map(|a| &a.event),
        Some(AutoEventView::Switched { to: 2, .. })
    ));
    h.assert_clean();
}

#[test]
fn an_exit_returns_to_the_default_profile_only_while_the_rule_holds_it() {
    let mut h = Harness::ready();
    rules(&h, &[("game.exe", 2)], Some(1));
    front(&h, 100, "game.exe");
    assert!(h.until(|w| w.active_profile == Some(2), 40));
    // Alt-tab away: nothing.
    front(&h, 7, "explorer.exe");
    h.ticks(60);
    assert_eq!(
        h.worker.active_profile,
        Some(2),
        "alt-tab away never switches"
    );
    let hd = handle(&h);
    assert_eq!(hd.auto_watched_pids(), [100]);
    hd.process_exited(100);
    h.ticks(20);
    assert_eq!(h.worker.active_profile, Some(2), "gone for 3 s first");
    assert!(h.until(|w| w.active_profile == Some(1), 60));
    assert_eq!(
        switched_to(&h),
        [
            (2, AutoCauseView::GameInFront),
            (1, AutoCauseView::GameExited)
        ]
    );
    assert_eq!(h.status().automation.holding, None);
    h.assert_clean();
}

#[test]
fn fn_k_on_the_keyboard_overrides_the_rule() {
    let mut h = Harness::ready();
    rules(&h, &[("game.exe", 2)], Some(1));
    front(&h, 100, "game.exe");
    assert!(h.until(|w| w.active_profile == Some(2), 40));
    // Fn + 3: the keyboard reports it; a manual switch.
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 3,
            after_ms: 0,
            report: true,
        })
        .unwrap();
    assert!(h.until(|w| w.active_profile == Some(3) && w.conn == Conn::Ready, 40));
    assert_eq!(h.status().automation.holding, None);
    let writes = h.writes();
    handle(&h).process_exited(100);
    h.ticks(100);
    assert_eq!(h.worker.active_profile, Some(3), "no return to the default");
    assert_eq!(h.writes(), writes);
    let state = handle(&h).auto_profile_state();
    assert!(
        state
            .recent
            .iter()
            .any(|a| matches!(a.event, AutoEventView::Overridden { profile: 3, .. }))
    );
}

#[test]
fn a_manual_switch_in_the_app_also_overrides_the_rule() {
    let mut h = Harness::ready();
    rules(&h, &[("game.exe", 2)], Some(1));
    front(&h, 100, "game.exe");
    assert!(h.until(|w| w.active_profile == Some(2), 40));
    h.ticks(60);
    let preview = h.prepare_switch(4).unwrap();
    h.apply_switch(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    handle(&h).process_exited(100);
    h.ticks(100);
    assert_eq!(h.worker.active_profile, Some(4));
}

#[test]
fn drafts_defer_the_switch_and_skip_it_after_30_s() {
    let mut h = Harness::ready();
    let shown = Arc::new(Recorder::<OsNotification>::default());
    handle(&h).set_notification_sink(shown.clone());
    rules(&h, &[("game.exe", 2)], None);
    handle(&h).report_ui_state(UiStateReport {
        drafts: true,
        vendor_acknowledged: true,
    });
    let writes = h.writes();
    front(&h, 100, "game.exe");
    h.ticks(20);
    let waiting = h.status().automation.waiting.expect("waiting");
    assert_eq!(waiting.reason, AutoWaitReason::Drafts);
    assert!(matches!(
        auto_notices(&h)[..],
        [Notice::AutoSwitchDeferred {
            reason: AutoWaitReason::Drafts,
            ..
        }]
    ));
    assert_eq!(
        shown.all().iter().map(|n| n.kind).collect::<Vec<_>>(),
        [NotificationKind::AutoSwitchWaiting]
    );
    h.ticks(300);
    assert!(matches!(
        auto_notices(&h).last(),
        Some(Notice::AutoSwitchSkipped {
            reason: AutoWaitReason::Drafts,
            ..
        })
    ));
    // Drafts gone: the episode stays skipped.
    handle(&h).report_ui_state(UiStateReport::default());
    h.ticks(80);
    assert_eq!(h.writes(), writes, "nothing was written");
    assert!(
        shown
            .all()
            .iter()
            .any(|n| n.kind == NotificationKind::AutoSwitchSkipped)
    );
}

#[test]
fn every_gate_of_the_worker_blocks_the_switch() {
    let mut h = Harness::ready();
    assert_eq!(h.worker.auto_block(), None);
    // A plan under review.
    let preview = h
        .prepare_trigger(h.trigger_intent(&[46], Some(60), Some(RtIntent::Off)))
        .unwrap();
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::PlanPending));
    h.post(Request::DiscardPlan {
        token: Harness::token(&preview.token),
    });
    assert_eq!(h.worker.auto_block(), None);
    // Drafts.
    handle(&h).report_ui_state(UiStateReport {
        drafts: true,
        vendor_acknowledged: false,
    });
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::Drafts));
    handle(&h).report_ui_state(UiStateReport::default());
    // An operation (admitted, queued).
    h.shared
        .busy
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::Busy));
    h.shared
        .busy
        .store(false, std::sync::atomic::Ordering::SeqCst);
    // The live key-depth view.
    h.shared.reserve_depth();
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::DepthView));
    h.shared.end_depth();
    // A key-depth test another run left open.
    h.shared.set_travel_marker_left(true);
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::TravelTest));
    h.shared.set_travel_marker_left(false);
    // A profile change being settled.
    h.worker.reread = true;
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::ProfileChanging));
    h.worker.reread = false;
    h.with_sim(|s| {
        s.leave_travel_open();
        s.travel_script(pos(46), &[(Duration::from_millis(50), 30)]);
    });
    h.ticks(3);
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::TravelTest));
    h.with_sim(|s| s.allow_travel_left_open());

    // Another program: blocked, then 2 minutes of memory after "check
    // again".
    let mut h = Harness::ready();
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 100,
        })
        .unwrap();
    h.ticks(5);
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::OtherProgram));
    h.op(|reply| Request::RecheckOtherProgram { reply })
        .unwrap();
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::Connecting));
    h.until_ready();
    assert_eq!(
        h.worker.auto_block(),
        Some(AutoWaitReason::RecentForeignActivity)
    );
    h.ticks(1200);
    assert_eq!(h.worker.auto_block(), None, "120 s later");

    // A reset, a released keyboard, none at all.
    let mut h = Harness::ready();
    h.sim.apply(SimAction::KeyboardReset).unwrap();
    h.tick();
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::ResetSeen));
    let mut h = Harness::ready();
    h.op(|reply| Request::Disconnect { reply }).unwrap();
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::Released));
    let mut h = Harness::new(0);
    h.ticks(2);
    assert_eq!(h.worker.auto_block(), Some(AutoWaitReason::NotConnected));
}

#[test]
fn the_switch_waits_for_the_keyboard_to_save_the_last_write() {
    let mut h = Harness::ready();
    rules(&h, &[("game.exe", 2)], None);
    // The game comes to the front while a trigger edit is written.
    let preview = h
        .prepare_trigger(h.trigger_intent(&[46], Some(60), Some(RtIntent::Off)))
        .unwrap();
    front(&h, 100, "game.exe");
    h.ticks(16);
    assert_eq!(
        h.status().automation.waiting.map(|w| w.reason),
        Some(AutoWaitReason::PlanPending)
    );
    h.apply_trigger(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    assert!(h.until(|w| w.active_profile == Some(2), 60));
    // The select came at least 2 s after the trigger write.
    let (packets, times) = h.with_sim(|s| (s.written().to_vec(), s.sent_times().to_vec()));
    let last_write = packets
        .iter()
        .zip(&times)
        .filter(|(p, _)| p.is_write() && p.as_bytes()[5] != 0x12)
        .map(|(_, t)| *t)
        .next_back()
        .unwrap();
    let select = packets
        .iter()
        .zip(&times)
        .find(|(p, _)| p.is_write() && p.as_bytes()[..6] == [0x21, 0, 0, 0, 0, 0x12])
        .map(|(_, t)| *t)
        .unwrap();
    assert!(
        select >= last_write + Duration::from_secs(2),
        "{select:?} {last_write:?}"
    );
    h.assert_clean();
}

#[test]
fn a_failed_switch_pauses_the_automation() {
    let mut h = Harness::ready();
    let shown = Arc::new(Recorder::<OsNotification>::default());
    handle(&h).set_notification_sink(shown.clone());
    let trays = Arc::new(Recorder::<TrayModel>::default());
    handle(&h).set_tray_sink(trays.clone());
    rules(&h, &[("game.exe", 2), ("other.exe", 3)], None);
    // The keyboard ignores the select (and the library's second try).
    h.with_sim(|s| s.ignore_next_selects(8));
    front(&h, 100, "game.exe");
    h.ticks(40);
    let paused = h.status().automation.paused;
    assert!(
        matches!(paused, Some(AutoPauseReason::SwitchFailed { .. })),
        "{paused:?}"
    );
    assert!(matches!(
        auto_notices(&h).last(),
        Some(Notice::AutoProfilePaused { .. })
    ));
    assert!(
        shown
            .all()
            .iter()
            .any(|n| n.kind == NotificationKind::AutoPaused)
    );
    let tray = trays.all().last().cloned().unwrap();
    assert!(tray.attention.contains(&TrayAttention::AutoPaused));
    // Nothing more until the user resumes.
    let writes = h.writes();
    front(&h, 101, "other.exe");
    h.ticks(80);
    assert_eq!(h.writes(), writes);
    handle(&h).pause_auto_profile(false);
    h.with_sim(|s| s.ignore_next_selects(0));
    assert!(h.until(|w| w.active_profile == Some(3), 80));
}

#[test]
fn the_tray_mode_rearms_after_180_s_of_silence_for_the_automation_only() {
    for tray in [true, false] {
        let mut h = Harness::ready();
        handle(&h)
            .set_background_settings(&BackgroundSettingsIntent {
                tray_enabled: tray,
                start_with_windows: false,
                notifications: crate::dto::NotificationSettings::default(),
                auto_backup: crate::dto::AutoBackupSettings::default(),
            })
            .unwrap();
        rules(&h, &[("game.exe", 2)], None);
        h.sim
            .apply(SimAction::VendorHeartbeat {
                count: 1,
                interval_ms: 100,
            })
            .unwrap();
        h.ticks(5);
        assert!(matches!(h.worker.conn, Conn::OtherProgram { .. }));
        let writes = h.writes();
        h.ticks(1700);
        assert!(!h.worker.auto_rearmed);
        front(&h, 100, "game.exe");
        h.ticks(350);
        if tray {
            assert!(h.worker.auto_rearmed, "180 s of silence");
            assert_eq!(h.worker.active_profile, Some(2), "switched");
            assert_eq!(h.writes(), writes + 1, "the select only");
            assert!(
                matches!(h.worker.conn, Conn::OtherProgram { .. }),
                "manual writes still need check again"
            );
            assert!(
                h.status()
                    .write_gate
                    .reasons
                    .contains(&WriteBlockReason::OtherProgram)
            );
            // Anything heard ends it.
            h.sim
                .apply(SimAction::VendorHeartbeat {
                    count: 1,
                    interval_ms: 100,
                })
                .unwrap();
            h.ticks(5);
            assert!(!h.worker.auto_rearmed);
        } else {
            assert!(!h.worker.auto_rearmed);
            assert_eq!(h.writes(), writes, "no switch without the tray");
            assert!(matches!(
                auto_notices(&h).last(),
                Some(Notice::AutoSwitchSkipped {
                    reason: AutoWaitReason::OtherProgram,
                    ..
                })
            ));
        }
        h.assert_clean();
    }
}

/// A ready harness's worker on its own thread (requests only).
fn threaded(h: Harness) -> (WorkerHandle, Arc<crate::shared::Shared>, tempfile::TempDir) {
    let Harness {
        mut worker,
        shared,
        sim,
        dir,
        tx,
        ..
    } = h;
    let config = worker.config.clone();
    worker.requests_only = true;
    let join = std::thread::spawn(move || worker.run());
    let handle = WorkerHandle::from_parts(tx, Arc::clone(&shared), config, Some(join), Some(sim));
    (handle, shared, dir)
}

#[test]
fn a_tray_profile_click_switches_only_when_nothing_stands_in_the_way() {
    let h = Harness::ready();
    let (handle, shared, _dir) = threaded(h);
    let open = |o: TrayOutcome| match o {
        TrayOutcome::OpenWindow { reason, .. } => Some(reason),
        _ => None,
    };
    // This run has no vendor-site acknowledgement yet.
    assert_eq!(
        open(handle.tray_action(TrayAction::SwitchProfile(2))),
        Some(OpenReason::NoVendorAcknowledgement)
    );
    handle.report_ui_state(UiStateReport {
        drafts: false,
        vendor_acknowledged: true,
    });
    assert_eq!(
        handle.tray_action(TrayAction::SwitchProfile(2)),
        TrayOutcome::Done
    );
    assert_eq!(shared.status().active_profile, Some(2));
    assert_eq!(
        handle.tray_action(TrayAction::SwitchProfile(2)),
        TrayOutcome::NothingToDo
    );
    handle.report_ui_state(UiStateReport {
        drafts: true,
        vendor_acknowledged: true,
    });
    assert_eq!(
        open(handle.tray_action(TrayAction::SwitchProfile(3))),
        Some(OpenReason::Drafts)
    );
    handle.report_ui_state(UiStateReport {
        drafts: false,
        vendor_acknowledged: true,
    });
    // A plan pending.
    let preview = handle.prepare_profile_switch(3).unwrap();
    assert_eq!(
        open(handle.tray_action(TrayAction::SwitchProfile(4))),
        Some(OpenReason::PlanPending)
    );
    handle.discard_plan(preview.token.unwrap());
    // "Back up now": an automatic backup of the active profile.
    assert_eq!(handle.tray_action(TrayAction::BackUpNow), TrayOutcome::Done);
    assert!(
        handle
            .list_auto_backups()
            .entries
            .iter()
            .any(|e| e.profile == 2)
    );
    // The attention items lead to their pages.
    assert!(matches!(
        handle.tray_action(TrayAction::Attention(TrayAttention::ResetReview)),
        TrayOutcome::OpenWindow {
            reason: OpenReason::NeedsWindow,
            ..
        }
    ));
    // Pause game profiles.
    assert_eq!(
        handle.tray_action(TrayAction::SetAutoPaused(true)),
        TrayOutcome::Done
    );
    assert_eq!(
        handle.auto_profile_state().status.paused,
        Some(AutoPauseReason::User)
    );
    // A reloaded window forgets the tick.
    handle.subscribe(Arc::new(Recorder::default()));
    assert_eq!(
        open(handle.tray_action(TrayAction::SwitchProfile(1))),
        Some(OpenReason::NoVendorAcknowledgement)
    );
    handle.shutdown();
}

#[test]
fn back_up_now_saves_a_manual_backup_unless_the_profile_looks_reset() {
    let mut h = Harness::ready();
    // A change made elsewhere is saved by the read's own comparison; an
    // unchanged profile reuses the newest backup; otherwise a manual one.
    let first = h.op(|reply| Request::BackUpNow { reply }).unwrap().unwrap();
    assert_eq!(first.profile, 1);
    assert_eq!(h.auto_files(1).len(), 1, "the same settings: no new file");
    // Held after a reset: nothing saved.
    h.with_sim(|s| s.factory_reset_all());
    h.sim.apply(SimAction::KeyboardReset).unwrap();
    h.tick();
    h.op(|reply| Request::Reconnect { reply }).unwrap();
    h.until_ready();
    assert_eq!(h.op(|reply| Request::BackUpNow { reply }).unwrap(), None);
    assert_eq!(h.auto_files(1).len(), 1);
}

#[test]
fn notifications_are_throttled_per_kind_and_follow_the_settings() {
    let mut h = Harness::ready();
    let shown = Arc::new(Recorder::<OsNotification>::default());
    let hd = handle(&h);
    hd.set_notification_sink(shown.clone());
    let switched = Notice::AutoSwitched {
        rule_id: Some("r-1".to_string()),
        label: Some("Game".to_string()),
        from: Some(1),
        to: 2,
        cause: AutoCauseView::GameInFront,
    };
    h.shared.notice(switched.clone());
    assert!(shown.all().is_empty(), "game switches are off by default");
    hd.set_background_settings(&BackgroundSettingsIntent {
        tray_enabled: true,
        start_with_windows: false,
        notifications: crate::dto::NotificationSettings {
            reset: true,
            auto_switch: true,
        },
        auto_backup: crate::dto::AutoBackupSettings::default(),
    })
    .unwrap();
    h.shared.notice(switched.clone());
    h.shared.notice(switched.clone());
    assert_eq!(shown.all().len(), 1, "one per kind per 10 minutes");
    // The keyboard's reset report is another kind.
    h.sim.apply(SimAction::KeyboardReset).unwrap();
    h.tick();
    assert_eq!(
        shown.all().last().map(|n| n.kind),
        Some(NotificationKind::KeyboardReset)
    );
    h.op(|reply| Request::Reconnect { reply }).unwrap();
    h.until_ready();
    h.ticks(6000);
    h.shared.notice(switched);
    assert_eq!(
        shown
            .all()
            .iter()
            .filter(|n| n.kind == NotificationKind::AutoSwitched)
            .count(),
        2,
        "again after 10 minutes"
    );
}

#[test]
fn programs_are_offered_by_token_and_rules_keep_their_programs() {
    let h = Harness::ready();
    let hd = handle(&h);
    assert!(hd.list_running_programs().is_empty(), "no source: no list");
    hd.set_program_source(Arc::new(FixedPrograms(vec![
        RunningProgram {
            pid: 10,
            exe: "cs2.exe".to_string(),
            path: Some(r"C:\Steam\cs2\cs2.exe".to_string()),
        },
        RunningProgram {
            pid: 11,
            exe: "javaw.exe".to_string(),
            path: Some(r"C:\Java\bin\javaw.exe".to_string()),
        },
    ])));
    let running = hd.list_running_programs();
    assert_eq!(running.len(), 2);
    let java = running.iter().find(|p| p.exe == "javaw.exe").unwrap();
    assert!(java.generic_name, "a generic name warns");
    let cs2 = running.iter().find(|p| p.exe == "cs2.exe").unwrap();
    assert_eq!(cs2.folder.as_deref(), Some(r"C:\Steam\cs2"));
    let picked = hd
        .register_program(std::path::Path::new(r"D:\Games\racer.exe"))
        .unwrap();
    // A file chosen by a roundabout path (a link, a junction, `..`) is
    // known by its final path, as Windows reports the running program.
    let real = h.dir.path().join("real");
    std::fs::create_dir_all(real.join("sub")).unwrap();
    std::fs::write(real.join("game.exe"), b"").unwrap();
    let via = hd
        .register_program(&real.join("sub").join("..").join("game.exe"))
        .unwrap();
    let folder = via.folder.unwrap_or_default();
    assert!(
        !folder.contains("..") && !folder.starts_with(r"\\?\"),
        "{folder}"
    );
    assert!(folder.ends_with("real"), "{folder}");
    let intent = AutoProfileRulesIntent {
        enabled: true,
        default_profile: Some(1),
        rules: vec![
            AutoProfileRuleIntent {
                id: None,
                label: "CS2".to_string(),
                program: RuleProgramIntent::Picked {
                    token: cs2.token.clone(),
                },
                profile: 2,
                enabled: true,
            },
            AutoProfileRuleIntent {
                id: None,
                label: "Minecraft".to_string(),
                program: RuleProgramIntent::Picked {
                    token: java.token.clone(),
                },
                profile: 3,
                enabled: true,
            },
            AutoProfileRuleIntent {
                id: None,
                label: "Racer".to_string(),
                program: RuleProgramIntent::Picked {
                    token: picked.token.clone(),
                },
                profile: 4,
                enabled: false,
            },
        ],
    };
    let state = hd.set_auto_profile_rules(&intent).unwrap();
    assert_eq!(state.rules.len(), 3);
    assert!(state.rules[1].generic_name);
    assert_eq!(state.rules[0].folder.as_deref(), Some(r"C:\Steam\cs2"));
    // Saved: a new start reads them; "keep" keeps the program.
    let stored = hd.auto_profile_settings();
    assert_eq!(stored.rules[2].path_hint.as_deref(), Some(r"D:\Games"));
    let keep = AutoProfileRulesIntent {
        enabled: true,
        default_profile: None,
        rules: vec![AutoProfileRuleIntent {
            id: Some("r-1".to_string()),
            label: "CS2 (comp)".to_string(),
            program: RuleProgramIntent::Keep,
            profile: 4,
            enabled: true,
        }],
    };
    let state = hd.set_auto_profile_rules(&keep).unwrap();
    assert_eq!(
        (state.rules[0].exe.as_str(), state.rules[0].profile),
        ("cs2.exe", 4)
    );
    // Refused: an unknown token, a path as a name.
    let bad = AutoProfileRulesIntent {
        rules: vec![AutoProfileRuleIntent {
            id: None,
            label: "X".to_string(),
            program: RuleProgramIntent::Name {
                exe: r"C:\x.exe".to_string(),
            },
            profile: 2,
            enabled: true,
        }],
        ..keep
    };
    assert_eq!(
        hd.set_auto_profile_rules(&bad).unwrap_err().kind,
        AppErrorKind::InvalidRequest
    );
    assert_eq!(hd.auto_profile_state().rules.len(), 1, "unchanged");
}

#[test]
fn detect_next_program_answers_with_the_next_program_in_front() {
    let h = Harness::ready();
    let hd = Arc::new(handle(&h));
    hd.foreground_changed(Some(ForegroundProgram {
        pid: 50,
        exe: Some("explorer.exe".to_string()),
        path: None,
        elevated: false,
    }));
    let waiter = {
        let hd = Arc::clone(&hd);
        std::thread::spawn(move || hd.detect_next_program(Duration::from_secs(10)))
    };
    for _ in 0..2000 {
        if h.shared.auto().detecting() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    // The program in front at the start is not the answer.
    hd.foreground_changed(Some(ForegroundProgram {
        pid: 50,
        exe: Some("explorer.exe".to_string()),
        path: None,
        elevated: false,
    }));
    assert!(h.shared.auto().detecting());
    hd.foreground_changed(Some(ForegroundProgram {
        pid: 60,
        exe: None,
        path: Some(r"C:\Games\new\game.exe".to_string()),
        elevated: false,
    }));
    let found = waiter.join().unwrap().expect("detected");
    assert_eq!(found.exe, "game.exe");
    assert_eq!(found.folder.as_deref(), Some(r"C:\Games\new"));
    // Nothing comes: `None` after the time out.
    assert_eq!(hd.detect_next_program(Duration::from_millis(20)), None);
}

/// The tray on (the 180 s rule applies).
fn tray(h: &Harness, on: bool) {
    handle(h)
        .set_background_settings(&BackgroundSettingsIntent {
            tray_enabled: on,
            start_with_windows: false,
            notifications: crate::dto::NotificationSettings::default(),
            auto_backup: crate::dto::AutoBackupSettings::default(),
        })
        .unwrap();
}

/// Another program heard once, then 190 s of silence: re-armed for the
/// automatic switches (tray mode).
fn rearmed_after_another_program(h: &mut Harness) {
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 100,
        })
        .unwrap();
    h.ticks(5);
    assert!(matches!(h.worker.conn, Conn::OtherProgram { .. }));
    h.ticks(1900);
    assert!(h.worker.auto_rearmed, "180 s of silence");
}

#[test]
fn a_profile_the_user_chose_stays_when_the_game_exits() {
    let mut h = Harness::ready();
    // The user picks profile 2 by hand; later a rule for it is saved.
    let preview = h.prepare_switch(2).unwrap();
    h.apply_switch(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    rules(&h, &[("game.exe", 2)], Some(1));
    front(&h, 100, "game.exe");
    h.ticks(40);
    assert_eq!(h.status().automation.holding, None, "no Auto chip");
    front(&h, 7, "explorer.exe");
    handle(&h).process_exited(100);
    h.ticks(100);
    assert_eq!(h.worker.active_profile, Some(2));
    assert_eq!(h.selects(), [2], "only the user's own switch");
    assert!(switched_to(&h).is_empty());
}

#[test]
fn two_games_on_one_profile_return_to_the_default_after_both_exit() {
    let mut h = Harness::ready();
    rules(&h, &[("a.exe", 2), ("b.exe", 2)], Some(1));
    front(&h, 100, "a.exe");
    assert!(h.until(|w| w.active_profile == Some(2), 40));
    front(&h, 101, "b.exe");
    h.ticks(30);
    front(&h, 7, "explorer.exe");
    let hd = handle(&h);
    hd.process_exited(100);
    h.ticks(60);
    assert_eq!(h.worker.active_profile, Some(2), "B still runs");
    hd.process_exited(101);
    assert!(h.until(|w| w.active_profile == Some(1), 80));
    assert_eq!(
        switched_to(&h),
        [
            (2, AutoCauseView::GameInFront),
            (1, AutoCauseView::GameExited)
        ]
    );
}

#[test]
fn fn_k_while_the_gate_is_rearmed_for_the_automation_still_wins() {
    let mut h = Harness::ready();
    tray(&h, true);
    rules(&h, &[("game.exe", 2)], Some(1));
    rearmed_after_another_program(&mut h);
    front(&h, 100, "game.exe");
    assert!(h.until(|w| w.active_profile == Some(2), 60));
    // The user's Fn + 3, reported by the keyboard.
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 3,
            after_ms: 0,
            report: true,
        })
        .unwrap();
    h.ticks(30);
    assert_eq!(h.worker.active_profile, Some(3));
    assert_eq!(h.status().automation.holding, None);
    // The game runs on, then exits: no return over the user's choice.
    h.ticks(1900);
    handle(&h).process_exited(100);
    h.ticks(80);
    assert_eq!(h.with_sim(|s| s.active_profile()), 3);
    assert_eq!(h.selects(), [2]);
}

#[test]
fn turning_the_tray_off_ends_the_rearm() {
    let mut h = Harness::ready();
    tray(&h, true);
    rules(&h, &[("game.exe", 2)], None);
    rearmed_after_another_program(&mut h);
    tray(&h, false);
    let writes = h.writes();
    front(&h, 100, "game.exe");
    h.ticks(350);
    assert!(!h.worker.auto_rearmed);
    assert_eq!(h.writes(), writes, "no switch without the tray");
    assert!(matches!(
        auto_notices(&h).last(),
        Some(Notice::AutoSwitchSkipped {
            reason: AutoWaitReason::OtherProgram,
            ..
        })
    ));
}

#[test]
fn a_reset_during_the_switch_gate_makes_the_switch_wait() {
    use openwraith_hid::protocol::{REPORT_LEN, Report};
    let mut h = Harness::ready();
    tray(&h, true);
    rules(&h, &[("game.exe", 2)], None);
    h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 100,
        })
        .unwrap();
    h.ticks(5);
    h.ticks(1700);
    assert!(!h.worker.auto_rearmed);
    let quiet = h.worker.other_quiet_since.unwrap();
    let rearm_at = quiet + Duration::from_secs(180);
    front(&h, 100, "game.exe");
    let until_rearm = rearm_at.saturating_duration_since(h.worker.now());
    // The keyboard's reset report 1.1 s into the switch's 2 s gate.
    let mut bytes = [0u8; REPORT_LEN];
    bytes[..6].copy_from_slice(&[0x14, 0, 0, 0, 0x01, 0x01]);
    h.with_sim(|s| {
        s.push_input_at(
            until_rearm + Duration::from_millis(1100),
            Report::new(bytes),
        )
    });
    let writes = h.writes();
    h.ticks(200);
    assert_eq!(h.worker.conn, Conn::ResetSeen);
    assert_eq!(h.writes(), writes, "nothing was sent");
    assert_eq!(h.status().automation.paused, None, "not a failed switch");
    assert!(
        auto_notices(&h)
            .iter()
            .all(|n| !matches!(n, Notice::AutoProfilePaused { .. }))
    );
}

#[test]
fn without_a_keyboard_a_game_waits_silently() {
    let mut h = Harness::new(0);
    h.ticks(3);
    let shown = Arc::new(Recorder::<OsNotification>::default());
    handle(&h).set_notification_sink(shown.clone());
    rules(&h, &[("game.exe", 2)], Some(1));
    front(&h, 100, "game.exe");
    h.ticks(400);
    assert!(shown.all().is_empty(), "{:?}", shown.all());
    assert!(auto_notices(&h).is_empty(), "{:?}", auto_notices(&h));
}

#[test]
fn fn_k_heard_while_the_app_observes_after_a_reconnect_is_a_manual_switch() {
    let mut h = Harness::ready();
    rules(&h, &[("game.exe", 2)], Some(1));
    h.op(|reply| Request::Reconnect { reply }).unwrap();
    assert_eq!(h.worker.conn, Conn::Observing);
    front(&h, 100, "game.exe");
    // Fn + 3 on the keyboard while the app observes, the game in front.
    h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 3,
            after_ms: 300,
            report: true,
        })
        .unwrap();
    h.until_ready();
    h.ticks(80);
    assert_eq!(h.worker.active_profile, Some(3), "the user's choice wins");
    assert!(
        h.selects().is_empty(),
        "nothing switched: {:?}",
        h.selects()
    );
    let state = handle(&h).auto_profile_state();
    assert!(
        state
            .recent
            .iter()
            .any(|a| matches!(a.event, AutoEventView::Overridden { profile: 3, .. })),
        "{:?}",
        state.recent
    );
    assert!(
        !state
            .recent
            .iter()
            .any(|a| matches!(a.event, AutoEventView::Switched { .. })),
        "{:?}",
        state.recent
    );
}

#[test]
fn fn_k_heard_during_the_read_after_a_reconnect_is_a_manual_switch() {
    let mut h = Harness::ready();
    rules(&h, &[("game.exe", 2)], Some(1));
    h.op(|reply| Request::Reconnect { reply }).unwrap();
    front(&h, 100, "game.exe");
    // Fn + 3 in the middle of the read after the 2 s observation (the read
    // starts again on the new profile).
    let base = h.packets();
    h.with_sim(|s| s.switch_profile_after_packets(base + 30, super::harness::profile(3)));
    h.until_ready();
    assert_eq!(h.snapshot().profile, 3);
    h.ticks(80);
    assert_eq!(h.worker.active_profile, Some(3), "the user's choice wins");
    assert!(
        h.selects().is_empty(),
        "nothing switched: {:?}",
        h.selects()
    );
    let state = handle(&h).auto_profile_state();
    assert!(
        state
            .recent
            .iter()
            .any(|a| matches!(a.event, AutoEventView::Overridden { profile: 3, .. })),
        "{:?}",
        state.recent
    );
}
