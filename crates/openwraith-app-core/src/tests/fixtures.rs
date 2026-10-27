//! The browser-preview fixtures (`app/src/preview/fixtures/*.ts`): every
//! scenario drives the real worker on the simulator (virtual time, fixed
//! tokens and times) and records the status, the snapshot, the pushed events
//! and each command call with its progress. Checked by default; rewritten
//! with `UPDATE_FIXTURES=1 cargo test -p openwraith-app-core fixtures`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use openwraith_hid::protocol::{Layer, MacroSlotId, REPORT_LEN, RawResponse, ReadRequest, Report};
use openwraith_hid::{Backup, BackupEntry, SimTable};
use serde::Serialize;
use serde_json::{Value, json};

use super::generated::{check_or_update, repo_root};
use super::harness::{Harness, Watch, pos, profile};
use super::seeds;
use crate::clock::{Clock, millis};
use crate::dto::{
    AppError, AppEvent, BackupKind, DeviceStatus, DialogText, FileRef, FileToken, OpInfo, OpKind,
    ProgressStep, RestoreIntent, RtIntent, SafetyBackupEntry, SimAction,
};
use crate::files::FileKind;
use crate::sink::EventSink;
use crate::worker::Request;

const W: u8 = 46;
const A: u8 = 68;
const S: u8 = 69;
const D: u8 = 70;
const Q: u8 = 45;
const E: u8 = 47;

/// Where the fixtures pretend files are.
const SAFETY_DIR: &str = r"C:\Users\you\AppData\Local\OpenWraith\safety-backups";
const USER_DIR: &str = r"C:\Users\you\Documents\OpenWraith";

/// Status events with their virtual time.
struct Stamped {
    clock: Arc<dyn Clock>,
    start: Mutex<Option<Instant>>,
    items: Mutex<Vec<(u32, AppEvent)>>,
}

impl EventSink for Stamped {
    fn send(&self, event: AppEvent) {
        let Some(start) = *self.start.lock().unwrap() else {
            return;
        };
        let at = millis(self.clock.now().saturating_duration_since(start));
        self.items.lock().unwrap().push((at, event));
    }
}

/// A scenario being recorded.
struct Rec {
    id: &'static str,
    h: Harness,
    sink: Arc<Stamped>,
    start: Option<Instant>,
    initial_status: Option<DeviceStatus>,
    initial_snapshot: Option<Value>,
    calls: Vec<Value>,
    safety_backups: Vec<SafetyBackupEntry>,
    sim_badge: bool,
    ui: Option<Value>,
}

fn dialog() -> DialogText {
    DialogText {
        title: "Choose a backup".to_string(),
        filter: "OpenWraith backups".to_string(),
    }
}

impl Rec {
    fn new(id: &'static str, h: Harness) -> Self {
        let sink = Arc::new(Stamped {
            clock: Arc::clone(&h.worker.clock),
            start: Mutex::new(None),
            items: Mutex::new(Vec::new()),
        });
        h.shared.set_subscriber(sink.clone());
        Rec {
            id,
            h,
            sink,
            start: None,
            initial_status: None,
            initial_snapshot: None,
            calls: Vec::new(),
            safety_backups: Vec::new(),
            sim_badge: true,
            ui: None,
        }
    }

    /// A ready keyboard.
    fn ready(id: &'static str) -> Self {
        Self::new(id, Harness::ready())
    }

    /// The page loads now: the initial status and snapshot; events and
    /// calls are timed from here.
    fn mark(&mut self) {
        self.h.worker.publish();
        let now = self.h.worker.now();
        self.start = Some(now);
        *self.sink.start.lock().unwrap() = Some(now);
        self.initial_status = Some(self.h.status());
        self.initial_snapshot = Some(serde_json::to_value(self.h.shared.snapshot()).unwrap());
    }

    fn at(&self) -> u32 {
        self.start.map_or(0, |s| {
            millis(self.h.worker.now().saturating_duration_since(s))
        })
    }

    /// Records a command call.
    fn call<T: Serialize>(
        &mut self,
        command: &str,
        args: Value,
        f: impl FnOnce(&mut Harness, &Arc<Watch>) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        if self.start.is_none() {
            self.mark();
        }
        let at_ms = self.at();
        let watch = self.h.watch();
        let result = f(&mut self.h, &watch);
        let (ok, err) = match &result {
            Ok(value) => (serde_json::to_value(value).unwrap(), Value::Null),
            Err(error) => (Value::Null, serde_json::to_value(error).unwrap()),
        };
        self.calls.push(json!({
            "command": command,
            "args": args,
            "ok": ok,
            "err": err,
            "progress": serde_json::to_value(watch.all()).unwrap(),
            "at_ms": at_ms,
        }));
        result
    }

    /// Ticks for `ms` of virtual time.
    fn wait(&mut self, ms: u64) {
        let ticks = usize::try_from(ms / 100).unwrap();
        self.h.ticks(ticks.max(1));
    }

    fn token(value: &Option<crate::dto::PlanToken>) -> crate::dto::PlanToken {
        value.clone().expect("a token")
    }

    /// The `subscribe` result as the first status, without the simulator
    /// badge for the plain scenarios.
    fn finish(mut self) -> (String, String) {
        if self.start.is_none() {
            self.mark();
        }
        let tmp = self.h.dir.path().to_path_buf();
        let mut app_info = serde_json::to_value(self.h.shared.app_info()).unwrap();
        let mut status = serde_json::to_value(self.initial_status.take().unwrap()).unwrap();
        let mut events: Vec<Value> = self
            .sink
            .items
            .lock()
            .unwrap()
            .iter()
            .map(|(at, e)| json!({"at_ms": at, "event": e}))
            .collect();
        if !self.sim_badge {
            app_info["sim"] = json!(false);
            status["sim"] = json!(false);
            for e in &mut events {
                if e["event"]["event"] == "status" {
                    e["event"]["status"]["sim"] = json!(false);
                }
            }
        }
        let mut fixture = json!({
            "id": self.id,
            "app_info": app_info,
            "settings": serde_json::to_value(crate::dto::Settings::default()).unwrap(),
            "initial_status": status,
            "initial_snapshot": self.initial_snapshot.take().unwrap_or(Value::Null),
            "events": events,
            "calls": self.calls,
            "safety_backups": serde_json::to_value(&self.safety_backups).unwrap(),
        });
        if let Some(ui) = self.ui.take() {
            fixture["ui"] = ui;
        }
        sanitize(&mut fixture, &tmp);
        let text = format!(
            "// This file was generated by the fixtures test of openwraith-app-core. Do not edit this file manually.\n\
             import type {{ Fixture }} from \"../types\";\n\n\
             export const fixture: Fixture = {};\n",
            layout(&fixture)
        );
        (format!("{}.ts", self.id), text)
    }

    /// The safety-backup listing (with fixed modification times).
    fn list_safety(&mut self) -> Vec<SafetyBackupEntry> {
        let dir = self.h.worker.config.safety_dir.clone().unwrap();
        crate::files::list_safety(&dir)
            .unwrap()
            .into_iter()
            .map(|(path, name, _)| {
                let file = self.h.shared.register_file(path, FileKind::Safety);
                let mut entry = crate::files::safety_entry(file, &name, None);
                entry.modified_at = Some("2026-10-24T07:30:00Z".to_string());
                entry
            })
            .collect()
    }

    /// `open_backup_file` of a file in the test directory.
    fn open_file(&mut self, path: PathBuf) -> Result<crate::dto::BackupFileSummary, AppError> {
        self.call("open_backup_file", json!({"dialog": dialog()}), |h, _| {
            let token = h.user_file(path.clone());
            let (backup, kind, path) = crate::ops::restore::load_backup(&h.shared, &token)?;
            let connected = h.shared.snapshot().map(|s| s.device);
            Ok(crate::ops::restore::summary_of(
                &token,
                &backup,
                kind,
                &path,
                connected.as_ref(),
            ))
        })
    }

    /// `choose_backup_destination`.
    fn choose_destination(&mut self, kind: BackupKind, name: &str) -> FileRef {
        let name = name.to_string();
        self.call(
            "choose_backup_destination",
            json!({"kind": kind, "dialog": dialog()}),
            |h, _| Ok(h.destination(&name)),
        )
        .unwrap()
    }
}

/// A fixture as a TypeScript literal: one line per field, and one line per
/// event and per call (compact JSON inside).
fn layout(fixture: &Value) -> String {
    let compact = |v: &Value| serde_json::to_string(v).unwrap();
    let mut out = String::from("{\n");
    if let Value::Object(map) = fixture {
        for (key, value) in map {
            match value {
                Value::Array(items) if !items.is_empty() => {
                    out.push_str(&format!("  {}: [\n", compact(&json!(key))));
                    for item in items {
                        out.push_str(&format!("    {},\n", compact(item)));
                    }
                    out.push_str("  ],\n");
                }
                _ => out.push_str(&format!(
                    "  {}: {},\n",
                    compact(&json!(key)),
                    compact(value)
                )),
            }
        }
    }
    out.push('}');
    out
}

/// Replaces the test directory in every string with fixed Windows paths.
fn sanitize(value: &mut Value, tmp: &std::path::Path) {
    let tmp = tmp.display().to_string();
    let safety = std::path::Path::new(&tmp)
        .join("safety")
        .display()
        .to_string();
    match value {
        Value::String(text) => {
            if text.contains(&tmp) {
                let replaced = text.replace(&safety, SAFETY_DIR).replace(&tmp, USER_DIR);
                *text = replaced.replace('/', "\\");
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|v| sanitize(v, std::path::Path::new(&tmp))),
        Value::Object(map) => map
            .values_mut()
            .for_each(|v| sanitize(v, std::path::Path::new(&tmp))),
        _ => {}
    }
}

// ----- the scenarios -----

fn shell() -> Vec<Rec> {
    let mut out = Vec::new();

    let mut r = Rec::new("searching", Harness::new(0));
    r.h.ticks(3);
    out.push(r);

    let mut r = Rec::new("several_keyboards", Harness::new(2));
    r.h.ticks(3);
    out.push(r);

    let mut r = Rec::new("observing", Harness::new(1));
    r.h.ticks(2);
    r.mark();
    r.wait(2500);
    out.push(r);

    let mut r = Rec::new("other_program", Harness::new(1));
    r.h.tick();
    r.h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 3,
            interval_ms: 500,
        })
        .unwrap();
    r.h.ticks(20);
    r.mark();
    r.call("recheck_other_program", json!({}), |h, _| {
        h.op(|reply| Request::RecheckOtherProgram { reply })
    })
    .unwrap();
    r.wait(3000);
    out.push(r);

    // The status while the settings are read (published during the read).
    let mut r = Rec::new("reading", Harness::new(1));
    r.h.ticks(5);
    r.mark();
    r.h.until_ready();
    let reading = r
        .sink
        .items
        .lock()
        .unwrap()
        .iter()
        .find_map(|(_, e)| match e {
            AppEvent::Status { status }
                if matches!(
                    status.connection,
                    crate::dto::ConnectionState::Reading { .. }
                ) =>
            {
                Some((**status).clone())
            }
            _ => None,
        })
        .expect("a reading status");
    // Halfway through (virtual time publishes only the first update).
    let mut reading = reading;
    if let crate::dto::ConnectionState::Reading { total, .. } = reading.connection {
        reading.connection = crate::dto::ConnectionState::Reading {
            done: total / 2,
            total,
        };
    }
    r.initial_status = Some(reading);
    r.initial_snapshot = Some(Value::Null);
    out.push(r);

    let mut r = Rec::ready("ready");
    r.sim_badge = false;
    out.push(r);

    out.push(Rec::ready("ready_sim"));

    let mut h = Harness::new(1);
    h.with_sim(|s| *s = seeds::firmware_sim("3.23.01"));
    h.until_ready();
    out.push(Rec::new("untested_firmware", h));

    let mut r = Rec::ready("reset_seen");
    r.h.sim.apply(SimAction::KeyboardReset).unwrap();
    r.h.tick();
    out.push(r);

    let mut h = Harness::new(1);
    h.with_sim(|s| *s = seeds::unsupported_sim());
    h.ticks(30);
    out.push(Rec::new("unsupported", h));

    let mut h = Harness::new(1);
    h.sim.fail_next_opens(0, 3);
    h.tick();
    out.push(Rec::new("open_failed", h));

    let mut r = Rec::ready("lost");
    r.h.sim.apply(SimAction::Unplug).unwrap();
    r.h.tick();
    out.push(r);

    let mut r = Rec::ready("released");
    r.call("disconnect", json!({}), |h, _| {
        h.op(|reply| Request::Disconnect { reply })
    })
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("fault");
    r.h.post(Request::Panic);
    out.push(r);

    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W], Some(60), None))
        .unwrap();
    h.apply_trigger(&preview.token.unwrap(), &h.watch())
        .unwrap();
    let safety = h.safety_files().remove(0);
    crate::ops::journal::write(
        &h.worker.config.journal_path(),
        &crate::ops::journal::JournalEntry {
            op: OpKind::Restore,
            started_at: "2026-10-24T07:30:00Z".into(),
            profiles: vec![1, 2],
            device: crate::ops::journal::JournalDevice {
                product: Some("W75HEARGB".into()),
                firmware: Some("3.22.07".into()),
            },
            safety_backup_path: safety.display().to_string(),
            app_version: "0.1.0".into(),
        },
    )
    .unwrap();
    let interrupted = crate::handle::interrupted(&h.shared, &h.worker.config);
    h.shared.set_interrupted(interrupted);
    out.push(Rec::new("interrupted", h));

    let mut r = Rec::ready("profile_switch");
    let preview = r
        .call("prepare_profile_switch", json!({"profile": 2}), |h, _| {
            h.prepare_switch(2)
        })
        .unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_profile_switch", json!({"token": token}), |h, w| {
        h.apply_switch(&token, w)
    })
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("profile_switch_refused");
    let preview = r
        .call("prepare_profile_switch", json!({"profile": 3}), |h, _| {
            h.prepare_switch(3)
        })
        .unwrap();
    r.h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 0,
        })
        .unwrap();
    let token = Rec::token(&preview.token);
    let _ = r.call("apply_profile_switch", json!({"token": token}), |h, w| {
        h.apply_switch(&token, w)
    });
    out.push(r);

    let mut r = Rec::ready("close_requested");
    let mut status = r.h.status();
    status.close_requested = true;
    status.busy = Some(OpInfo {
        op: crate::dto::OpId("o-9".into()),
        kind: OpKind::Restore,
        last: Some(crate::dto::ProgressEvent {
            op: crate::dto::OpId("o-9".into()),
            seq: 14,
            elapsed_ms: 3200,
            step: ProgressStep::WaitingForKeyboardSave {
                profile: Some(2),
                wait_ms: 1800,
            },
            cancel: crate::dto::CancelState::NotNow,
        }),
    });
    status.write_gate.allowed = false;
    status.write_gate.reasons = vec![crate::dto::WriteBlockReason::Busy];
    r.mark();
    r.initial_status = Some(status);
    out.push(r);

    out
}

fn actuation() -> Vec<Rec> {
    let mut out = Vec::new();
    let mixed = |id: &'static str| {
        let mut h = Harness::new(1);
        h.with_sim(|s| {
            for key in [W, A, S, D] {
                s.set_trigger(profile(1), pos(key), [0x0C, 60, 60, 15, 15, 0, 0, 0, 0]);
            }
            s.set_trigger(profile(1), pos(Q), [0x0D, 85, 85, 15, 25, 0, 0, 0, 0]);
            // Written by other software: 0C with different press and release.
            s.set_trigger(profile(1), pos(E), [0x0C, 85, 85, 15, 25, 0, 0, 0, 0]);
        });
        h.until_ready();
        Rec::new(id, h)
    };
    out.push(mixed("trigger_ready_mixed"));

    let preview_of = |r: &mut Rec, keys: &[u8], actuation: Option<u16>, rt: Option<RtIntent>| {
        let intent = r.h.trigger_intent(keys, actuation, rt);
        r.call(
            "prepare_trigger_edit",
            json!({ "intent": intent.clone() }),
            |h, _| h.prepare_trigger(intent),
        )
    };

    let mut r = Rec::ready("trigger_preview");
    preview_of(
        &mut r,
        &[W, A, S, D],
        Some(60),
        Some(RtIntent::Same { sensitivity: 15 }),
    )
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("trigger_apply_ok");
    let preview = preview_of(
        &mut r,
        &[W, A, S, D],
        Some(60),
        Some(RtIntent::Same { sensitivity: 15 }),
    )
    .unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_trigger_edit", json!({"token": token}), |h, w| {
        h.apply_trigger(&token, w)
    })
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("trigger_apply_maybe_lost");
    let preview = preview_of(&mut r, &[W, A], Some(60), None).unwrap();
    let token = Rec::token(&preview.token);
    let sim = r.h.sim.clone();
    r.call("apply_trigger_edit", json!({"token": token}), |h, w| {
        let mut done = false;
        w.on_event(move |event, _| {
            if !done && matches!(event.step, ProgressStep::WaitingForKeyboardSave { .. }) {
                done = true;
                sim.apply(SimAction::KeyboardSwitch {
                    profile: 2,
                    after_ms: 300,
                    report: true,
                })
                .unwrap();
            }
        });
        h.apply_trigger(&token, w)
    })
    .unwrap();
    r.wait(1000);
    out.push(r);

    let mut r = Rec::ready("trigger_nothing_to_do");
    preview_of(
        &mut r,
        &[W],
        Some(50),
        Some(RtIntent::Same { sensitivity: 15 }),
    )
    .unwrap();
    out.push(r);

    let mut h = Harness::new(1);
    h.with_sim(|s| *s = crate::backend::sim::sim_from(&seeds::other_unit_backup(), "SIM-1"));
    h.until_ready();
    let mut r = Rec::new("trigger_unit_mismatch", h);
    let _ = preview_of(&mut r, &[W], Some(60), None);
    out.push(r);

    let mut r = Rec::ready("trigger_stale_plan");
    let preview = preview_of(&mut r, &[W], Some(60), Some(RtIntent::Off)).unwrap();
    r.h.sim
        .apply(SimAction::KeyboardSwitch {
            profile: 2,
            after_ms: 0,
            report: false,
        })
        .unwrap();
    let token = Rec::token(&preview.token);
    let _ = r.call("apply_trigger_edit", json!({"token": token}), |h, w| {
        h.apply_trigger(&token, w)
    });
    out.push(r);

    let mut r = Rec::ready("trigger_refused_other_program");
    let preview = preview_of(&mut r, &[W], Some(60), None).unwrap();
    r.h.sim
        .apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 0,
        })
        .unwrap();
    let token = Rec::token(&preview.token);
    let _ = r.call("apply_trigger_edit", json!({"token": token}), |h, w| {
        h.apply_trigger(&token, w)
    });
    r.wait(300);
    out.push(r);

    out
}

fn lighting() -> Vec<Rec> {
    let mut out = Vec::new();
    let preview_of =
        |r: &mut Rec, mode: Option<u8>, color: Option<&str>, brightness: Option<u8>| {
            let intent = r.h.light_intent(mode, color, brightness, None);
            r.call(
                "prepare_light_edit",
                json!({ "intent": intent.clone() }),
                |h, _| h.prepare_light(intent),
            )
        };
    out.push(Rec::ready("light_ready"));

    let mut r = Rec::ready("light_preview_mode_change");
    preview_of(&mut r, Some(0x00), Some("#0000ff"), None).unwrap();
    out.push(r);

    let mut r = Rec::ready("light_apply_ok");
    let preview = preview_of(&mut r, Some(0x00), Some("#0000ff"), None).unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_light_edit", json!({"token": token}), |h, w| {
        h.apply_light(&token, w)
    })
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("light_apply_brightness_only");
    let preview = preview_of(&mut r, None, None, Some(2)).unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_light_edit", json!({"token": token}), |h, w| {
        h.apply_light(&token, w)
    })
    .unwrap();
    out.push(r);

    let mut h = Harness::new(1);
    h.with_sim(|s| {
        s.set_table(
            profile(1),
            SimTable::MainLight,
            vec![0x05, 0x03, 0x02, 0xff, 0x80, 0x00, 0, 0, 0, 0, 0, 0],
        );
    });
    h.until_ready();
    out.push(Rec::new("light_unknown_mode", h));

    let mut h = Harness::new(1);
    h.with_sim(|s| {
        s.set_table(
            profile(1),
            SimTable::MainLight,
            vec![0x00, 0x04, 0x02, 0x00, 0x80, 0xff, 0, 0, 0, 0, 0, 1],
        );
    });
    h.until_ready();
    out.push(Rec::new("light_power_off", h));

    let mut r = Rec::ready("light_apply_verify_failed");
    r.h.with_sim(|s| s.freeze(SimTable::MainLight));
    let preview = preview_of(&mut r, None, None, Some(1)).unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_light_edit", json!({"token": token}), |h, w| {
        h.apply_light(&token, w)
    })
    .unwrap();
    out.push(r);

    out
}

/// Replaces the reply to `request` in the snapshot of `profile` of an
/// all-profile backup with `table` (chunked like the keyboard sends it).
fn replace_table(
    backup: &mut Backup,
    profile: u8,
    request: ReadRequest,
    cmd: u8,
    sub: u8,
    table: &[u8],
) {
    let layout = request.table_layout().unwrap();
    let reports: Vec<Report> = (0..layout.chunks)
        .map(|chunk| {
            let start = usize::from(chunk) * layout.chunk_len;
            let len = layout.expected_len(chunk);
            let mut bytes = [0u8; REPORT_LEN];
            let [hi, lo] = chunk.to_be_bytes();
            bytes[..5].copy_from_slice(&[cmd, sub, hi, lo, u8::try_from(len).unwrap()]);
            bytes[5..5 + len].copy_from_slice(&table[start..start + len]);
            Report::new(bytes)
        })
        .collect();
    let raw = RawResponse::from_reports(request, reports).unwrap();
    let entries = if backup.active_profile == Some(profile) {
        &mut backup.entries
    } else {
        &mut backup
            .profiles
            .iter_mut()
            .find(|p| p.profile == profile)
            .unwrap()
            .entries
    };
    let entry = entries.iter_mut().find(|e| e.request() == request).unwrap();
    *entry = BackupEntry::new(raw);
}

/// A ready keyboard with an all-profile backup file whose restore writes
/// triggers, the light and the base keymap (so the Fn keymap is re-sent) and
/// skips profile 2's keymap group (a non-empty advanced key).
fn restore_setup(id: &'static str) -> (Rec, PathBuf) {
    let mut h = Harness::ready();
    let (_, path) = h.backup_all("all.owbackup.json");
    let mut backup = Backup::load(&path).unwrap();
    // Profile 2's backup holds an advanced key the keyboard does not.
    let mut advanced = vec![0u8; 1600];
    advanced[..4].copy_from_slice(&[0x01, 0x2C, 0x00, 0x04]);
    replace_table(
        &mut backup,
        2,
        ReadRequest::AdvancedKeys,
        0x22,
        0x80,
        &advanced,
    );
    let edited = h.dir.path().join("restore.owbackup.json");
    backup.save(&edited, true).unwrap();
    h.with_sim(|s| {
        s.set_trigger(
            profile(1),
            pos(W),
            [0x00, 0x55, 0x55, 0x01, 0x01, 0, 0, 0, 0],
        );
        s.set_table(
            profile(2),
            SimTable::MainLight,
            vec![0x00, 0x01, 0x02, 0x00, 0xff, 0x00, 0, 0, 0, 0, 0, 0],
        );
        let mut base = s
            .table(profile(1), SimTable::Keymap(Layer::Base))
            .unwrap()
            .to_vec();
        base[usize::from(W) * 4 + 2] ^= 0x01;
        s.set_table(profile(1), SimTable::Keymap(Layer::Base), base);
    });
    (Rec::new(id, h), edited)
}

fn restore_intent(file: &FileToken, profiles: Option<Vec<u8>>, switches: Vec<u8>) -> RestoreIntent {
    RestoreIntent {
        source: file.clone(),
        profiles,
        allow_firmware_mismatch: false,
        confirmed_switches: switches,
    }
}

impl Rec {
    fn prepare_restore(
        &mut self,
        intent: RestoreIntent,
    ) -> Result<crate::dto::RestorePreview, AppError> {
        self.call(
            "prepare_restore",
            json!({ "intent": intent.clone() }),
            |h, w| h.prepare_restore(intent, w),
        )
    }

    fn apply_restore(
        &mut self,
        token: &crate::dto::PlanToken,
    ) -> Result<crate::dto::RestoreReportView, AppError> {
        let token = token.clone();
        self.call("apply_restore", json!({"token": token}), |h, w| {
            h.apply_restore(&token, w)
        })
    }
}

fn backup_restore() -> Vec<Rec> {
    let mut out = Vec::new();

    let mut r = Rec::ready("backup_active_ok");
    let destination = r.choose_destination(BackupKind::Active, "active.owbackup.json");
    r.call(
        "backup_active_profile",
        json!({"destination": destination.token}),
        |h, w| h.backup_active(&destination, w),
    )
    .unwrap();
    out.push(r);

    let all = |id: &'static str| {
        let mut r = Rec::ready(id);
        let destination = r.choose_destination(BackupKind::AllProfiles, "all.owbackup.json");
        let token = destination.token.clone();
        let preview = r
            .call(
                "prepare_all_profile_backup",
                json!({"destination": token}),
                |h, _| h.prepare_all(&destination),
            )
            .unwrap();
        (r, preview)
    };
    let (r, _) = all("backup_all_preview");
    out.push(r);

    let (mut r, preview) = all("backup_all_ok");
    r.call(
        "apply_all_profile_backup",
        json!({"token": preview.token}),
        |h, w| h.apply_all(&preview.token, w),
    )
    .unwrap();
    out.push(r);

    let (mut r, preview) = all("backup_all_cancelled");
    let _ = r.call(
        "apply_all_profile_backup",
        json!({"token": preview.token}),
        |h, w| {
            w.cancel_when(|e| {
                matches!(
                    e.step,
                    ProgressStep::Reading {
                        profile: Some(3),
                        ..
                    }
                )
            });
            h.apply_all(&preview.token, w)
        },
    );
    out.push(r);

    let mut r = Rec::ready("safety_list");
    let preview =
        r.h.prepare_trigger(r.h.trigger_intent(&[W], Some(60), None))
            .unwrap();
    r.h.apply_trigger(&preview.token.unwrap(), &r.h.watch())
        .unwrap();
    let preview =
        r.h.prepare_light(r.h.light_intent(None, None, Some(2), None))
            .unwrap();
    r.h.apply_light(&preview.token.unwrap(), &r.h.watch())
        .unwrap();
    let listed = r.list_safety();
    r.safety_backups = listed.clone();
    r.call("list_safety_backups", json!({}), |_, _| Ok(listed))
        .unwrap();
    out.push(r);

    let (mut r, path) = restore_setup("restore_summary_all");
    r.open_file(path).unwrap();
    out.push(r);

    let (mut r, path) = restore_setup("restore_preview");
    let summary = r.open_file(path).unwrap();
    r.prepare_restore(restore_intent(&summary.file.token, None, vec![]))
        .unwrap_err();
    r.prepare_restore(restore_intent(&summary.file.token, None, vec![2, 3, 4]))
        .unwrap();
    out.push(r);

    let (mut r, path) = restore_setup("restore_ok_persisted");
    let summary = r.open_file(path).unwrap();
    let preview = r
        .prepare_restore(restore_intent(&summary.file.token, None, vec![2, 3, 4]))
        .unwrap();
    r.apply_restore(&Rec::token(&preview.token)).unwrap();
    out.push(r);

    let (mut r, path) = restore_setup("restore_lost");
    r.h.sim.apply(SimAction::SlowSave { ms: 5000 }).unwrap();
    let summary = r.open_file(path).unwrap();
    let preview = r
        .prepare_restore(restore_intent(&summary.file.token, Some(vec![2]), vec![2]))
        .unwrap();
    r.apply_restore(&Rec::token(&preview.token)).unwrap();
    out.push(r);

    let (mut r, path) = restore_setup("restore_maybe_lost_disconnect");
    let summary = r.open_file(path).unwrap();
    let preview = r
        .prepare_restore(restore_intent(&summary.file.token, Some(vec![1]), vec![]))
        .unwrap();
    let token = Rec::token(&preview.token);
    let sim = r.h.sim.clone();
    r.call("apply_restore", json!({"token": token}), |h, w| {
        let mut done = false;
        w.on_event(move |event, _| {
            if !done && matches!(event.step, ProgressStep::WaitingForKeyboardSave { .. }) {
                done = true;
                sim.apply(SimAction::Unplug).unwrap();
            }
        });
        h.apply_restore(&token, w)
    })
    .unwrap();
    r.wait(500);
    out.push(r);

    let mut h = Harness::ready();
    let (_, path) = h.backup_all("all.owbackup.json");
    h.with_sim(|s| {
        let mut base = s
            .table(profile(1), SimTable::Keymap(Layer::Base))
            .unwrap()
            .to_vec();
        base[usize::from(W) * 4 + 2] ^= 0x01;
        s.set_table(profile(1), SimTable::Keymap(Layer::Base), base);
        let mut slot = vec![0u8; 256];
        slot[..8].copy_from_slice(&[0, 0, 0, 4, 0x01, 0x04, 0x00, 0x00]);
        s.set_table(
            profile(1),
            SimTable::Macro(MacroSlotId::new(0).unwrap()),
            slot,
        );
    });
    let mut r = Rec::new("restore_cancelled_in_keymap_group", h);
    let summary = r.open_file(path).unwrap();
    let preview = r
        .prepare_restore(restore_intent(&summary.file.token, Some(vec![1]), vec![]))
        .unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_restore", json!({"token": token}), |h, w| {
        w.cancel_when(super::harness::is_write_start);
        h.apply_restore(&token, w)
    })
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("restore_firmware_mismatch");
    let newer = r.h.dir.path().join("newer.owbackup.json");
    seeds::with_device_info("W75HEARGB", "3.23.01")
        .save(&newer, false)
        .unwrap();
    let summary = r.open_file(newer).unwrap();
    r.prepare_restore(restore_intent(&summary.file.token, None, vec![]))
        .unwrap_err();
    r.prepare_restore(RestoreIntent {
        allow_firmware_mismatch: true,
        ..restore_intent(&summary.file.token, None, vec![])
    })
    .unwrap();
    out.push(r);

    let (mut r, path) = restore_setup("restore_needs_switch_consent");
    let summary = r.open_file(path).unwrap();
    r.prepare_restore(restore_intent(&summary.file.token, None, vec![]))
        .unwrap_err();
    out.push(r);

    let mut h = Harness::ready();
    let (_, path) = h.backup_all("all.owbackup.json");
    let mut r = Rec::new("restore_nothing_to_write", h);
    let summary = r.open_file(path).unwrap();
    r.prepare_restore(restore_intent(&summary.file.token, None, vec![2, 3, 4]))
        .unwrap();
    out.push(r);

    let mut h = Harness::ready();
    let preview = h
        .prepare_trigger(h.trigger_intent(&[W, A], Some(100), Some(RtIntent::Off)))
        .unwrap();
    let report = h
        .apply_trigger(&preview.token.unwrap(), &h.watch())
        .unwrap();
    let mut r = Rec::new("undo_preview", h);
    let file = report.safety_backup.token.clone();
    r.call("open_safety_backup", json!({"file": file}), |h, _| {
        let (backup, kind, path) = crate::ops::restore::load_backup(&h.shared, &file)?;
        let connected = h.shared.snapshot().map(|s| s.device);
        Ok(crate::ops::restore::summary_of(
            &file,
            &backup,
            kind,
            &path,
            connected.as_ref(),
        ))
    })
    .unwrap();
    r.prepare_restore(restore_intent(&report.safety_backup.token, None, vec![]))
        .unwrap();
    r.ui = Some(json!({ "restore": report.safety_backup.token }));
    out.push(r);

    let mut r = Rec::ready("restore_invalid_file");
    let bad = r.h.dir.path().join("broken.owbackup.json");
    std::fs::write(&bad, "{ \"format\": \"something else\" }").unwrap();
    r.open_file(bad).unwrap_err();
    let newer = r.h.dir.path().join("newer-schema.owbackup.json");
    std::fs::write(
        &newer,
        "{ \"format\": \"openwraith-backup\", \"schema_version\": 9 }",
    )
    .unwrap();
    let error = r.open_file(newer).unwrap_err();
    assert!(matches!(
        error.detail,
        Some(crate::dto::ErrorDetail::UnsupportedSchema { found: 9, .. })
    ));
    out.push(r);

    out
}

/// Every scenario, rendered.
fn render_all() -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let mut ids = Vec::new();
    for rec in shell()
        .into_iter()
        .chain(actuation())
        .chain(lighting())
        .chain(backup_restore())
    {
        ids.push(rec.id);
        let (name, text) = rec.finish();
        assert!(files.insert(name, text).is_none(), "duplicate fixture");
    }
    ids.sort_unstable();
    let mut index = String::from(
        "// This file was generated by the fixtures test of openwraith-app-core. Do not edit this file manually.\n\
         import type { Fixture } from \"../types\";\n\n",
    );
    for id in &ids {
        index.push_str(&format!("import {{ fixture as {id} }} from \"./{id}\";\n"));
    }
    index.push_str("\n/** Every recorded scenario, by id. */\nexport const fixtures: Record<string, Fixture> = {\n");
    for id in &ids {
        index.push_str(&format!("  {id},\n"));
    }
    index.push_str("};\n\n/** The scenario ids. */\nexport const fixtureIds: readonly string[] = Object.keys(fixtures);\n");
    files.insert("index.ts".to_string(), index);
    files
}

#[test]
fn fixtures_are_up_to_date() {
    let files = render_all();
    assert!(files.len() >= 45, "{} files", files.len());
    check_or_update(
        &repo_root().join("app/src/preview/fixtures"),
        "ts",
        &files,
        "UPDATE_FIXTURES",
        "UPDATE_FIXTURES=1 cargo test -p openwraith-app-core fixtures",
    );
}

#[test]
fn fixtures_are_deterministic() {
    let a = render_all();
    let b = render_all();
    for (name, text) in &a {
        assert_eq!(Some(text), b.get(name), "{name} differs between two runs");
    }
    // No path of the test machine leaks into a fixture.
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    if user.len() > 3 {
        for (name, text) in &a {
            assert!(!text.contains(&user), "{name} contains the user name");
        }
    }
}
