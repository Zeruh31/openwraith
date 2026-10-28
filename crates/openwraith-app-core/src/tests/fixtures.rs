//! The browser-preview fixtures (`app/src/preview/fixtures/*.ts`): every
//! scenario drives the real worker on the simulator (virtual time, fixed
//! tokens and times) and records the status, the snapshot, the pushed events
//! and each command call with its progress. Checked by default; rewritten
//! with `UPDATE_FIXTURES=1 cargo test -p openwraith-app-core fixtures`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use openwraith_hid::protocol::{Layer, REPORT_LEN, RawResponse, ReadRequest, Report};
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
    h.with_sim(super::harness::base_keymap_and_advanced_slot_changed);
    let mut r = Rec::new("restore_cancelled_in_keymap_group", h);
    let summary = r.open_file(path).unwrap();
    let preview = r
        .prepare_restore(restore_intent(&summary.file.token, Some(vec![1]), vec![]))
        .unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_restore", json!({"token": token}), |h, w| {
        w.cancel_when(super::harness::is_base_keymap_start);
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

impl Rec {
    /// `get_keys_state` (a direct command: the view of the last full read).
    fn keys_state(&mut self) -> crate::dto::KeysState {
        self.call("get_keys_state", json!({}), |h, _| Ok(h.keys()))
            .unwrap()
    }

    fn prepare_keymap(
        &mut self,
        intent: crate::dto::KeymapEditIntent,
    ) -> Result<crate::dto::KeymapEditPreview, AppError> {
        self.call(
            "prepare_keymap_edit",
            json!({ "intent": intent.clone() }),
            |h, _| h.prepare_keymap(intent),
        )
    }

    fn apply_keymap(
        &mut self,
        token: &crate::dto::PlanToken,
        on: impl FnOnce(&Watch),
    ) -> Result<crate::dto::EditReportView, AppError> {
        let token = token.clone();
        self.call("apply_keymap_edit", json!({"token": token}), |h, w| {
            on(w);
            h.apply_keymap(&token, w)
        })
    }
}

/// A ready keyboard whose keymap group holds every kind of entry and slot
/// (`tests::keymap::keys_seed`).
fn keys_seeded(id: &'static str) -> Rec {
    Rec::new(id, super::keymap::seeded())
}

/// A ready keyboard with macros in slots 1 and 2 bound to Q (base) and W
/// (Fn).
fn keys_two_macros(id: &'static str) -> Rec {
    let mut h = Harness::new(1);
    h.with_sim(super::keymap::two_macros);
    h.until_ready();
    Rec::new(id, h)
}

/// The Keys page's scenarios.
fn keys() -> Vec<Rec> {
    use super::keymap::{CAPS, E, F2, GRAVE, R, TWO, tap_view};
    use crate::dto::{
        ActionValue, AdvancedDefView, BuiltinAckIntent, DksRowView, DksStepView,
        KeymapChangeIntent, LayerView, MouseActionView, TableRef,
    };
    let mut out = Vec::new();

    // The three tabs: remaps on both layers, Fn built-ins, a macro on the
    // Fn layer, an orphan macro, an entry kept as read, DKS / Mod-Tap /
    // Toggle, a slot of another type, a broken reference; the library of
    // this PC names slot 2's macro.
    let mut r = keys_seeded("keys_ready");
    r.keys_state();
    r.call("list_macro_library", json!({}), |h, _| {
        crate::library::save(&h.worker.config, "Type A", &tap_view(0x04, 30))?;
        crate::library::save(&h.worker.config, "Copy", &tap_view(0x06, 0))?;
        crate::library::list(&h.worker.config)
    })
    .unwrap();
    out.push(r);

    // Remaps on both layers, a non-critical built-in replaced (acknowledged)
    // and a swap.
    let assign_intent = |h: &Harness| {
        let mut intent = h.keymap_intent(vec![
            KeymapChangeIntent::Assign {
                layer: LayerView::Base,
                key: R,
                action: ActionValue::Combo {
                    modifiers: vec![0xE0],
                    key: 0x06,
                },
            },
            KeymapChangeIntent::Assign {
                layer: LayerView::Fn,
                key: F2,
                action: ActionValue::Mouse {
                    action: MouseActionView::Back,
                },
            },
            KeymapChangeIntent::Swap {
                layer: LayerView::Base,
                first: CAPS,
                second: GRAVE,
            },
        ]);
        intent.builtin_acks = vec![BuiltinAckIntent {
            key: F2,
            critical: false,
        }];
        intent
    };
    let mut r = keys_seeded("keys_preview_assign");
    r.keys_state();
    let intent = assign_intent(&r.h);
    r.prepare_keymap(intent).unwrap();
    out.push(r);

    let mut r = keys_seeded("keys_apply_ok");
    r.keys_state();
    let intent = assign_intent(&r.h);
    let preview = r.prepare_keymap(intent).unwrap();
    r.apply_keymap(&Rec::token(&preview.token), |_| {}).unwrap();
    r.keys_state();
    out.push(r);

    // With "check that the keyboard kept it".
    let mut r = keys_seeded("keys_apply_checked");
    r.keys_state();
    let mut intent = r.h.keymap_intent(vec![KeymapChangeIntent::BindMacro {
        layer: LayerView::Fn,
        key: R,
        program: tap_view(0x05, 50),
        slot: None,
    }]);
    intent.check_persistence = true;
    let preview = r.prepare_keymap(intent).unwrap();
    r.apply_keymap(&Rec::token(&preview.token), |_| {}).unwrap();
    r.keys_state();
    out.push(r);

    // Fn + 2 without the critical acknowledgement: refused.
    let mut r = keys_seeded("keys_refused_critical");
    r.keys_state();
    let intent = r.h.keymap_intent(vec![KeymapChangeIntent::Assign {
        layer: LayerView::Fn,
        key: TWO,
        action: ActionValue::Key { usage: 0x04 },
    }]);
    r.prepare_keymap(intent).unwrap_err();
    out.push(r);

    // Advanced slot 5 holds a kind OpenWraith keeps as read: replacing it
    // is refused until the user agrees that Undo can't bring it back.
    let mut r = keys_seeded("keys_refused_not_undoable");
    r.keys_state();
    let toggle = |h: &Harness, allow: bool| {
        let mut intent = h.keymap_intent(vec![KeymapChangeIntent::SetAdvanced {
            slot: 5,
            def: AdvancedDefView::Toggle { key: 0x0B },
        }]);
        intent.allow_not_undoable = allow;
        intent
    };
    let intent = toggle(&r.h, false);
    r.prepare_keymap(intent).unwrap_err();
    let intent = toggle(&r.h, true);
    r.prepare_keymap(intent).unwrap();
    out.push(r);

    // A DKS bound to a key: the unchanged Fn keymap before the slot; a tap
    // at stage 4 is not yet verified.
    let dks_on_e = |h: &Harness| {
        h.keymap_intent(vec![KeymapChangeIntent::BindAdvanced {
            layer: LayerView::Base,
            key: E,
            def: AdvancedDefView::Dks {
                first_point: 12,
                rows: vec![
                    Some(DksRowView {
                        key: 0x08,
                        steps: vec![
                            DksStepView::Tap,
                            DksStepView::None,
                            DksStepView::None,
                            DksStepView::Tap,
                        ],
                    }),
                    Some(DksRowView {
                        key: 0x15,
                        steps: vec![
                            DksStepView::None,
                            DksStepView::Hold,
                            DksStepView::None,
                            DksStepView::None,
                        ],
                    }),
                    None,
                    None,
                ],
            },
            slot: None,
        }])
    };
    let mut r = Rec::ready("keys_preview_advanced");
    r.keys_state();
    let intent = dks_on_e(&r.h);
    r.prepare_keymap(intent).unwrap();
    out.push(r);

    // The same DKS written: base keymap, the Fn keymap that commits it, the
    // slot (written even after Stop, it completes the binding).
    let mut r = Rec::ready("keys_apply_advanced");
    r.keys_state();
    let intent = dks_on_e(&r.h);
    let preview = r.prepare_keymap(intent).unwrap();
    r.apply_keymap(&Rec::token(&preview.token), |_| {}).unwrap();
    r.keys_state();
    out.push(r);

    // A content-only change of an advanced key (the seeded Mod-Tap on G,
    // held 400 ms: not yet verified): the Fn keymap goes first, so the
    // keyboard keeps the slot (commit trigger "advanced_keys").
    let mut r = keys_seeded("keys_preview_advanced_edit");
    r.keys_state();
    let intent = r.h.keymap_intent(vec![KeymapChangeIntent::SetAdvanced {
        slot: 1,
        def: AdvancedDefView::ModTap {
            tap: 0x0a,
            hold: 0xe0,
            hold_ms: 400,
        },
    }]);
    r.prepare_keymap(intent).unwrap();
    out.push(r);

    // Macro slot 0: slots 1 and 2 are re-sent right after it.
    let slot0 = |h: &Harness| {
        h.keymap_intent(vec![
            KeymapChangeIntent::SetMacro {
                slot: 0,
                program: tap_view(0x06, 10),
            },
            KeymapChangeIntent::Assign {
                layer: LayerView::Base,
                key: GRAVE,
                action: ActionValue::Key { usage: 0x29 },
            },
        ])
    };
    let mut r = keys_two_macros("keys_preview_slot0");
    r.keys_state();
    let intent = slot0(&r.h);
    r.prepare_keymap(intent).unwrap();
    out.push(r);

    // "New macro…" on a key that runs Macro 1 on its own (R here): the
    // library reuses Macro 1 rather than a free slot, so slots 1 and 2 are
    // re-sent right after it.
    let mut h = Harness::new(1);
    h.with_sim(super::keymap::two_macros);
    h.until_ready();
    let first = h.keymap_intent(vec![KeymapChangeIntent::BindMacro {
        layer: LayerView::Base,
        key: R,
        program: tap_view(0x06, 10),
        slot: Some(0),
    }]);
    let preview = h.prepare_keymap(first).unwrap();
    h.apply_keymap(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    let mut r = Rec::new("keys_preview_new_on_macro1", h);
    r.keys_state();
    let intent = r.h.keymap_intent(vec![KeymapChangeIntent::BindMacro {
        layer: LayerView::Base,
        key: R,
        program: tap_view(0x07, 10),
        slot: None,
    }]);
    let preview = r.prepare_keymap(intent).unwrap();
    assert!(
        preview
            .slot_changes
            .iter()
            .any(|c| c.slot == TableRef::Macro { slot: 0 }),
        "the new macro goes into Macro 1"
    );
    out.push(r);

    // Stop right after slot 0: the re-sends still go out, the unrelated
    // layer change does not.
    let mut r = keys_two_macros("keys_apply_cancelled_after_slot0");
    r.keys_state();
    let intent = slot0(&r.h);
    let preview = r.prepare_keymap(intent).unwrap();
    r.apply_keymap(&Rec::token(&preview.token), |w| {
        w.cancel_when(|e| {
            matches!(
                e.step,
                ProgressStep::Writing {
                    table: TableRef::Macro { slot: 0 },
                    packet,
                    packets,
                    ..
                } if packet == packets && packet > 0
            )
        });
    })
    .unwrap();
    r.keys_state();
    out.push(r);

    // The re-send of slot 1 fails: slots 1 and 2 are left empty, Q and Fn +
    // W refer to them.
    let mut r = keys_two_macros("keys_apply_failed_cleared");
    r.h.with_sim(|s| {
        s.freeze(SimTable::Macro(
            openwraith_hid::protocol::MacroSlotId::new(1).unwrap(),
        ));
    });
    r.keys_state();
    let intent = r.h.keymap_intent(vec![KeymapChangeIntent::SetMacro {
        slot: 0,
        program: tap_view(0x06, 10),
    }]);
    let preview = r.prepare_keymap(intent).unwrap();
    r.apply_keymap(&Rec::token(&preview.token), |_| {}).unwrap();
    r.keys_state();
    out.push(r);

    // A library file that cannot be read is kept aside.
    let mut r = Rec::ready("keys_library_invalid");
    r.call("list_macro_library", json!({}), |h, _| {
        std::fs::write(crate::library::path(&h.worker.config), "{ not json").unwrap();
        crate::library::list(&h.worker.config)
    })
    .unwrap();
    out.push(r);

    out
}

// ----- colours, dead band, SOCD, presets -----

impl Rec {
    fn colors_state(&mut self) -> crate::dto::ColorsState {
        self.call("get_colors_state", json!({}), |h, _| Ok(h.colors()))
            .unwrap()
    }

    fn dead_band_state(&mut self) -> crate::dto::DeadBandState {
        self.call("get_dead_band_state", json!({}), |h, _| Ok(h.dead_band()))
            .unwrap()
    }

    fn socd_state(&mut self) -> crate::dto::SocdStateView {
        self.call("get_socd_state", json!({}), |h, _| Ok(h.socd()))
            .unwrap()
    }

    fn list_presets(&mut self) -> Result<crate::dto::PresetsView, AppError> {
        self.call("list_presets", json!({}), |h, _| {
            let limits = h.snapshot().triggers.limits;
            crate::ops::presets::list(&h.worker.config, limits)
        })
    }
}

fn color_intent(r: &Rec, keys: &[(u8, &str)], switch: bool) -> crate::dto::ColorEditIntent {
    crate::dto::ColorEditIntent {
        colors: keys
            .iter()
            .map(|(key, color)| crate::dto::KeyColorIntent {
                key: *key,
                color: (*color).to_string(),
            })
            .collect(),
        switch_to_custom: switch,
        check_persistence: false,
        based_on: r.h.colors().revision,
    }
}

fn dead_band_intent(
    r: &Rec,
    keys: &[u8],
    top: Option<u16>,
    bottom: Option<u16>,
) -> crate::dto::DeadBandEditIntent {
    crate::dto::DeadBandEditIntent {
        keys: keys.to_vec(),
        top,
        bottom,
        check_persistence: false,
        based_on: r.h.dead_band().revision,
    }
}

fn socd_intent(
    r: &Rec,
    power: Option<bool>,
    pairs: Option<&[(u8, u8, u8)]>,
) -> crate::dto::SocdEditIntent {
    crate::dto::SocdEditIntent {
        power,
        pairs: pairs.map(|p| {
            p.iter()
                .map(|&(key1, key2, mode)| crate::dto::SocdPairIntent { key1, key2, mode })
                .collect()
        }),
        allow_not_undoable: false,
        check_persistence: false,
        based_on: r.h.socd().revision,
    }
}

/// A ready keyboard whose light is in the factory mode (not Custom).
fn static_light(id: &'static str) -> Rec {
    let mut h = Harness::new(1);
    h.with_sim(|s| {
        s.set_table(
            profile(1),
            SimTable::MainLight,
            openwraith_hid::sim::FACTORY_LIGHT.to_vec(),
        );
    });
    h.until_ready();
    Rec::new(id, h)
}

/// The editors of the Actuation and Lighting pages.
fn editors() -> Vec<Rec> {
    let mut out = Vec::new();

    // Key colours.
    let mut r = Rec::ready("colors_ready");
    r.colors_state();
    out.push(r);

    let mut r = static_light("colors_preview_not_shown");
    r.colors_state();
    let intent = color_intent(&r, &[(W, "#00ff00"), (A, "#00ff00")], false);
    r.call(
        "prepare_color_edit",
        json!({ "intent": intent.clone() }),
        |h, _| h.prepare_colors(intent),
    )
    .unwrap();
    out.push(r);

    let mut r = static_light("colors_preview_switch");
    r.colors_state();
    let intent = color_intent(&r, &[(W, "#00ff00"), (A, "#00ff00")], true);
    r.call(
        "prepare_color_edit",
        json!({ "intent": intent.clone() }),
        |h, _| h.prepare_colors(intent),
    )
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("colors_apply_ok");
    r.colors_state();
    let intent = color_intent(
        &r,
        &[
            (W, "#0000ff"),
            (A, "#0000ff"),
            (S, "#0000ff"),
            (D, "#0000ff"),
        ],
        false,
    );
    let preview = r
        .call(
            "prepare_color_edit",
            json!({ "intent": intent.clone() }),
            |h, _| h.prepare_colors(intent),
        )
        .unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_color_edit", json!({"token": token}), |h, w| {
        h.apply_colors(&token, w)
    })
    .unwrap();
    r.colors_state();
    out.push(r);

    // Dead band.
    let mut r = Rec::ready("dead_band_ready");
    r.dead_band_state();
    out.push(r);

    let mut r = Rec::ready("dead_band_preview");
    r.dead_band_state();
    let intent = dead_band_intent(&r, &[W, A, S, D], Some(50), Some(10));
    r.call(
        "prepare_dead_band_edit",
        json!({ "intent": intent.clone() }),
        |h, _| h.prepare_dead_band(intent),
    )
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("dead_band_apply_ok");
    r.dead_band_state();
    let intent = dead_band_intent(&r, &[W, A, S, D], Some(5), Some(5));
    let preview = r
        .call(
            "prepare_dead_band_edit",
            json!({ "intent": intent.clone() }),
            |h, _| h.prepare_dead_band(intent),
        )
        .unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_dead_band_edit", json!({"token": token}), |h, w| {
        h.apply_dead_band(&token, w)
    })
    .unwrap();
    r.dead_band_state();
    out.push(r);

    let mut r = Rec::ready("dead_band_refused_range");
    r.dead_band_state();
    let intent = dead_band_intent(&r, &[W], Some(51), None);
    r.call(
        "prepare_dead_band_edit",
        json!({ "intent": intent.clone() }),
        |h, _| h.prepare_dead_band(intent),
    )
    .unwrap_err();
    out.push(r);

    // SOCD.
    let mut r = Rec::ready("socd_ready");
    r.socd_state();
    out.push(r);

    let mut h = Harness::new(1);
    h.with_sim(|s| {
        let mut pairs = vec![0u8; 80];
        pairs[..12].copy_from_slice(&[
            0x01, 0x00, 0x04, 0x07, 0x01, 0x01, 0x1a, 0x16, 0x01, 0x03, 0x50, 0x4f,
        ]);
        s.set_table(profile(1), SimTable::SocdPairs, pairs);
        let mut settings = s
            .table(profile(1), SimTable::SocdSettings)
            .unwrap()
            .to_vec();
        settings[0] = 1;
        s.set_table(profile(1), SimTable::SocdSettings, settings);
    });
    h.until_ready();
    let mut r = Rec::new("socd_ready_pairs", h);
    r.socd_state();
    out.push(r);

    let mut r = Rec::ready("socd_preview");
    r.socd_state();
    let intent = socd_intent(&r, Some(true), Some(&[(A, D, 1), (W, S, 2), (Q, E, 3)]));
    r.call(
        "prepare_socd_edit",
        json!({ "intent": intent.clone() }),
        |h, _| h.prepare_socd(intent),
    )
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("socd_apply_ok");
    r.socd_state();
    let intent = socd_intent(&r, Some(true), Some(&[(A, D, 1)]));
    let preview = r
        .call(
            "prepare_socd_edit",
            json!({ "intent": intent.clone() }),
            |h, _| h.prepare_socd(intent),
        )
        .unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_socd_edit", json!({"token": token}), |h, w| {
        h.apply_socd(&token, w)
    })
    .unwrap();
    r.socd_state();
    out.push(r);

    // Fn + Right Shift on the keyboard while a SOCD plan is open.
    let mut r = Rec::ready("socd_toggled");
    r.socd_state();
    let intent = socd_intent(&r, None, Some(&[(A, D, 4)]));
    r.call(
        "prepare_socd_edit",
        json!({ "intent": intent.clone() }),
        |h, _| h.prepare_socd(intent),
    )
    .unwrap();
    r.h.with_sim(|s| s.press_socd_hotkey_at(std::time::Duration::from_millis(50)));
    r.wait(300);
    r.socd_state();
    out.push(r);

    // Presets: the built-in table, a user preset and one outside the
    // keyboard's limits (refused, never clamped).
    let mut r = Rec::ready("presets_ready");
    let path = r.h.worker.config.settings_path.clone().unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{"language":"system","theme":"system","presets":[
            {"id":"u-1","name":"Valorant","actuation":30,"rapid_trigger":{"mode":"same","sensitivity":8}},
            {"id":"u-2","name":"Too deep","actuation":250,"rapid_trigger":{"mode":"off"}}]}"#,
    )
    .unwrap();
    r.list_presets().unwrap();
    out.push(r);

    let mut r = Rec::ready("presets_preview");
    r.list_presets().unwrap();
    let intent = crate::dto::PresetEditIntent {
        preset: crate::dto::PresetRef::Builtin {
            id: "fast".to_string(),
        },
        keys: vec![W, A, S, D],
        based_on: r.h.snapshot().revision,
    };
    r.call(
        "prepare_preset_edit",
        json!({ "intent": intent.clone() }),
        |h, _| h.prepare_preset(intent),
    )
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("presets_save_refused");
    r.list_presets().unwrap();
    let preset = crate::dto::UserPresetIntent {
        id: None,
        name: "Too deep".to_string(),
        actuation: 250,
        rapid_trigger: RtIntent::Off,
    };
    r.call(
        "save_user_preset",
        json!({ "preset": preset.clone() }),
        |h, _| {
            let limits = h.snapshot().triggers.limits;
            crate::ops::presets::save(&h.worker.config, &preset, limits)
        },
    )
    .unwrap_err();
    out.push(r);

    // A preset of the user's own, saved (it is listed after the built-ins).
    let mut r = Rec::ready("presets_save_ok");
    r.list_presets().unwrap();
    let preset = crate::dto::UserPresetIntent {
        id: None,
        name: "Valorant".to_string(),
        actuation: 30,
        rapid_trigger: RtIntent::Same { sensitivity: 8 },
    };
    r.call(
        "save_user_preset",
        json!({ "preset": preset.clone() }),
        |h, _| {
            let limits = h.snapshot().triggers.limits;
            crate::ops::presets::save(&h.worker.config, &preset, limits)
        },
    )
    .unwrap();
    out.push(r);

    // A built-in preset applied: its plan is a trigger plan, written by the
    // trigger edit's apply step.
    let mut r = Rec::ready("presets_apply_ok");
    r.list_presets().unwrap();
    let intent = crate::dto::PresetEditIntent {
        preset: crate::dto::PresetRef::Builtin {
            id: "fast".to_string(),
        },
        keys: vec![W, A, S, D],
        based_on: r.h.snapshot().revision,
    };
    let preview = r
        .call(
            "prepare_preset_edit",
            json!({ "intent": intent.clone() }),
            |h, _| h.prepare_preset(intent),
        )
        .unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_trigger_edit", json!({"token": token}), |h, w| {
        h.apply_trigger(&token, w)
    })
    .unwrap();
    out.push(r);

    out
}

// ----- the live key-depth view -----

/// A hook on frames (`false`: the receiver is gone).
type FrameHook = Box<dyn FnMut(&crate::dto::DepthFrame, &crate::shared::Shared) -> bool + Send>;

/// Records frames; the hook runs on each.
struct FrameRecorder {
    frames: Mutex<Vec<crate::dto::DepthFrame>>,
    hook: Mutex<FrameHook>,
    shared: Arc<crate::shared::Shared>,
}

impl crate::sink::DepthSink for FrameRecorder {
    fn send(&self, frame: crate::dto::DepthFrame) -> bool {
        let keep = {
            let mut hook = self.hook.lock().unwrap();
            (*hook)(&frame, &self.shared)
        };
        self.frames.lock().unwrap().push(frame);
        keep
    }
}

/// W, A and D move (a slow press, a quick tap, a hold).
fn script_keys(h: &Harness) {
    use std::time::Duration;
    h.with_sim(|s| {
        s.travel_script(
            pos(W),
            &[
                (Duration::from_millis(100), 15),
                (Duration::from_millis(172), 40),
                (Duration::from_millis(244), 80),
                (Duration::from_millis(316), 120),
                (Duration::from_millis(388), 60),
            ],
        );
        s.travel_script(
            pos(A),
            &[
                (Duration::from_millis(200), 30),
                (Duration::from_millis(202), 150),
                (Duration::from_millis(204), 70),
            ],
        );
        s.travel_script(pos(D), &[(Duration::from_millis(300), 90)]);
    });
}

impl Rec {
    /// `start_depth_view`; the frames go to `ui.depth_frames`.
    fn depth_view(
        &mut self,
        hook: impl FnMut(&crate::dto::DepthFrame, &crate::shared::Shared) -> bool + Send + 'static,
    ) -> Result<crate::dto::DepthViewReport, AppError> {
        let sink = Arc::new(FrameRecorder {
            frames: Mutex::new(Vec::new()),
            hook: Mutex::new(Box::new(hook)),
            shared: Arc::clone(&self.h.shared),
        });
        let inner = Arc::clone(&sink);
        let result = self.call("start_depth_view", json!({}), |h, _| h.depth_view(inner));
        let frames = sink.frames.lock().unwrap().clone();
        self.ui = Some(json!({ "depth_frames": frames }));
        result
    }
}

/// The Key depth tab.
fn depth() -> Vec<Rec> {
    use crate::dto::DepthStopReason;
    let mut out = Vec::new();

    // Live frames; Stop after W reached its deepest point.
    let mut r = Rec::ready("depth_live");
    script_keys(&r.h);
    r.depth_view(|frame, shared| {
        // W is back at rest.
        if frame.keys.iter().any(|k| k.index == W && k.depth == 0) {
            shared.stop_depth(DepthStopReason::Requested);
        }
        true
    })
    .unwrap();
    out.push(r);

    for (id, reason) in [
        ("depth_stopped_page_left", DepthStopReason::PageLeft),
        ("depth_stopped_window_hidden", DepthStopReason::WindowHidden),
    ] {
        let mut r = Rec::ready(id);
        script_keys(&r.h);
        r.depth_view(move |_, shared| {
            shared.stop_depth(reason);
            true
        })
        .unwrap();
        out.push(r);
    }

    let mut r = Rec::ready("depth_stopped_no_movement");
    r.depth_view(|_, _| true).unwrap();
    out.push(r);

    let mut r = Rec::ready("depth_stopped_channel_closed");
    script_keys(&r.h);
    r.depth_view(|_, _| false).unwrap();
    out.push(r);

    let mut r = Rec::ready("depth_stopped_reset");
    script_keys(&r.h);
    let sim = r.h.sim.clone();
    r.depth_view(move |_, _| {
        sim.apply(SimAction::KeyboardReset).unwrap();
        true
    })
    .unwrap();
    r.h.with_sim(|s| s.allow_travel_left_open());
    out.push(r);

    let mut r = Rec::ready("depth_stopped_other_program");
    script_keys(&r.h);
    let sim = r.h.sim.clone();
    r.depth_view(move |_, _| {
        sim.apply(SimAction::VendorHeartbeat {
            count: 1,
            interval_ms: 0,
        })
        .unwrap();
        true
    })
    .unwrap();
    out.push(r);

    let mut r = Rec::ready("depth_stopped_profile_changed");
    script_keys(&r.h);
    let sim = r.h.sim.clone();
    r.depth_view(move |_, _| {
        sim.apply(SimAction::KeyboardSwitch {
            profile: 2,
            after_ms: 0,
            report: true,
        })
        .unwrap();
        true
    })
    .unwrap();
    r.wait(1000);
    out.push(r);

    // A live view an earlier run left open: the marker at start, the notice,
    // and "Stop it".
    let h = Harness::new(1);
    let marker = h.worker.config.travel_marker_path();
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "{}").unwrap();
    h.shared
        .set_travel_marker_left(crate::ops::travel::marker_left(&h.worker.config));
    h.with_sim(|s| s.leave_travel_open());
    let mut r = Rec::new("depth_stray_marker", h);
    r.mark();
    r.h.until_ready();
    r.wait(300);
    r.call("close_stray_travel_test", json!({}), |h, _| h.close_stray())
        .unwrap();
    out.push(r);

    out
}

// ----- sharing a profile -----

impl Rec {
    fn export(
        &mut self,
        profile: u8,
        name: &str,
        switch: bool,
    ) -> Result<crate::dto::ExportedProfile, AppError> {
        let name = name.to_string();
        let file = self
            .call(
                "choose_export_destination",
                json!({"profile": profile, "dialog": dialog()}),
                |h, _| Ok(Some(h.destination(&name))),
            )
            .unwrap()
            .unwrap();
        let intent = crate::dto::ExportIntent {
            source: crate::dto::ExportSource::Keyboard,
            profile,
            label: Some("Tournament".to_string()),
            destination: file.token,
            confirmed_switch: switch,
        };
        self.call(
            "export_profile",
            json!({ "intent": intent.clone() }),
            |h, w| h.export(intent, w),
        )
    }

    fn open_import(&mut self, path: PathBuf) -> crate::dto::ImportSummaryView {
        self.call("open_import_file", json!({"dialog": dialog()}), |h, _| {
            crate::ops::share::import_summary(&h.shared, &path).map(Some)
        })
        .unwrap()
        .unwrap()
    }

    fn prepare_import(
        &mut self,
        intent: crate::dto::ImportIntent,
    ) -> Result<crate::dto::ImportPreview, AppError> {
        self.call(
            "prepare_import",
            json!({ "intent": intent.clone() }),
            |h, w| h.prepare_import(intent, w),
        )
    }
}

/// A profile file of profile 1 with a macro, its Fn + 2 and Fn + Space
/// replaced (critical built-ins: profile 2, reset) when `critical`; the
/// keyboard back as before.
fn share_file(h: &mut Harness, critical: bool) -> PathBuf {
    share_file_with(h, critical, |_| {})
}

/// [`share_file`], with `tweak` applied to the simulator before the export.
fn share_file_with(
    h: &mut Harness,
    critical: bool,
    tweak: impl FnOnce(&mut openwraith_hid::DeviceSim),
) -> PathBuf {
    use crate::dto::{ActionValue, BuiltinAckIntent, KeymapChangeIntent, LayerView};
    let mut changes = vec![KeymapChangeIntent::BindMacro {
        layer: LayerView::Base,
        key: super::keymap::CAPS,
        program: super::keymap::tap_view(0x04, 20),
        slot: None,
    }];
    let replaced = if critical {
        vec![(super::keymap::TWO, 0x04), (super::keymap::SPACE, 0x05)]
    } else {
        Vec::new()
    };
    changes.extend(
        replaced
            .iter()
            .map(|(key, usage)| KeymapChangeIntent::Assign {
                layer: LayerView::Fn,
                key: *key,
                action: ActionValue::Key { usage: *usage },
            }),
    );
    let mut intent = h.keymap_intent(changes);
    intent.builtin_acks = replaced
        .iter()
        .map(|(key, _)| BuiltinAckIntent {
            key: *key,
            critical: true,
        })
        .collect();
    let preview = h.prepare_keymap(intent).unwrap();
    h.apply_keymap(&Harness::token(&preview.token), &h.watch())
        .unwrap();
    h.with_sim(tweak);
    let destination = h.destination("shared.owprofile.json");
    h.export(
        crate::dto::ExportIntent {
            source: crate::dto::ExportSource::Keyboard,
            profile: 1,
            label: Some("Tournament".to_string()),
            destination: destination.token,
            confirmed_switch: false,
        },
        &h.watch(),
    )
    .unwrap();
    h.dir.path().join("shared.owprofile.json")
}

fn import_of(
    summary: &crate::dto::ImportSummaryView,
    include: crate::dto::ShareSelectionView,
    allowed_critical_keys: Vec<u8>,
) -> crate::dto::ImportIntent {
    crate::dto::ImportIntent {
        source: summary.file.file.token.clone(),
        from: None,
        into: 2,
        include,
        allowed_critical_keys,
        allow_firmware_mismatch: false,
        confirmed_switches: vec![2],
    }
}

/// Share a profile (Backup & profiles).
fn share() -> Vec<Rec> {
    let mut out = Vec::new();

    let mut r = Rec::ready("export_done");
    r.export(1, "profile-1.owprofile.json", false).unwrap();
    out.push(r);

    let mut r = Rec::ready("export_needs_switch");
    r.export(3, "profile-3.owprofile.json", false).unwrap_err();
    r.export(3, "profile-3.owprofile.json", true).unwrap();
    out.push(r);

    let mut h = Harness::ready();
    let path = share_file(&mut h, false);
    let mut r = Rec::new("import_summary", h);
    r.open_import(path);
    out.push(r);

    let mut h = Harness::ready();
    let path = share_file(&mut h, false);
    let mut r = Rec::new("import_preview", h);
    let summary = r.open_import(path);
    r.prepare_import(import_of(
        &summary,
        crate::ops::share::default_selection(),
        Vec::new(),
    ))
    .unwrap();
    out.push(r);

    let mut h = Harness::ready();
    let path = share_file(&mut h, true);
    let mut r = Rec::new("import_refused_critical", h);
    let summary = r.open_import(path);
    let keys = crate::dto::ShareSelectionView {
        keys: true,
        ..crate::ops::share::default_selection()
    };
    r.prepare_import(import_of(&summary, keys, Vec::new()))
        .unwrap();
    // Both keys allowed: the keys are imported, both callouts stay.
    r.prepare_import(import_of(
        &summary,
        keys,
        vec![super::keymap::TWO, super::keymap::SPACE],
    ))
    .unwrap();
    out.push(r);

    // The file replaces Fn + 2 and Fn + Space, but its base W holds an entry
    // OpenWraith does not write: the keys stay out whatever the permission,
    // so no shortcut is named and the general note explains it.
    let mut h = Harness::ready();
    let path = share_file_with(&mut h, true, |s| {
        let mut base = s
            .table(profile(1), SimTable::Keymap(Layer::Base))
            .unwrap()
            .to_vec();
        let at = usize::from(super::keymap::W) * 4;
        base[at..at + 4].copy_from_slice(&[0x55, 0, 0, 0]);
        s.set_table(profile(1), SimTable::Keymap(Layer::Base), base);
    });
    let mut r = Rec::new("import_keys_blocked_other", h);
    let summary = r.open_import(path);
    let keys = crate::dto::ShareSelectionView {
        keys: true,
        ..crate::ops::share::default_selection()
    };
    r.prepare_import(import_of(&summary, keys, Vec::new()))
        .unwrap();
    out.push(r);

    let mut h = Harness::ready();
    let path = share_file(&mut h, false);
    let mut r = Rec::new("import_apply_ok", h);
    let summary = r.open_import(path);
    let keys = crate::dto::ShareSelectionView {
        keys: true,
        ..crate::ops::share::default_selection()
    };
    let preview = r
        .prepare_import(import_of(&summary, keys, Vec::new()))
        .unwrap();
    let token = Rec::token(&preview.token);
    r.call("apply_import", json!({"token": token}), |h, w| {
        h.apply_import(&token, w)
    })
    .unwrap();
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
        .chain(keys())
        .chain(editors())
        .chain(depth())
        .chain(share())
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
