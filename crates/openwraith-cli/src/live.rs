//! `openwraith travel`: the live key-depth view (the keyboard's travel
//! test), which is always closed again; `openwraith travel --close`; and
//! `openwraith monitor`, which only listens.
//!
//! The travel test changes no stored setting, but while it is open the
//! keyboard streams the depth of every key that moves, until it is closed.
//! So the test is opened only through the library's guard
//! ([`live_travel`]), which closes it on every way out (the end of
//! `--duration`, Ctrl+C, a profile switch on the keyboard, another program,
//! an error); Ctrl+C only asks to stop ([`Ui::enable_cancel_close_first`]).
//! Only a reset notification or a lost connection leave it possibly open:
//! the output then names `openwraith travel --close`.

use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::time::Duration;

use openwraith_hid::{
    CloseResult, ConsentOrigin, ListenError, Session, SessionStats, Transport, TravelFrame,
    TravelOptions, TravelOutcome, TravelState, TravelStop, WriteConsent, WriteTransport,
    live_travel, read_summary,
};
use openwraith_protocol::{
    DeviceEvent, Distance, HEADER_LEN, HexBytes, KeyPos, Model, Report, TravelReport, TravelVariant,
};

use crate::args::{KeyUse, MonitorArgs, TravelArgs};
use crate::commands::{CLOSE_SITE_NOTE, Exit, Globals, UsageError, report_session};
use crate::device::Connector;
use crate::report::seconds_text;
use crate::table::Table;
use crate::ui::Ui;

/// At most this many keys are shown on the live line.
const LIVE_KEYS: usize = 3;

/// Width of a depth bar.
const BAR: u16 = 10;

/// What was seen of one key.
#[derive(Debug, Clone, Copy, Default)]
struct KeyStats {
    reports: u64,
    peak: u16,
    last: u16,
}

/// A depth in trigger units as millimetres.
fn mm(units: u16) -> String {
    Distance::from_units(units).to_string()
}

/// `W [####------] 0.80 mm`.
fn bar(name: &str, depth: u16, full: u16) -> String {
    let filled = (u32::from(depth) * u32::from(BAR) / u32::from(full.max(1))).min(u32::from(BAR));
    let filled = usize::try_from(filled).unwrap_or(0);
    format!(
        "{name} [{}{}] {}",
        "#".repeat(filled),
        "-".repeat(usize::from(BAR) - filled),
        mm(depth)
    )
}

/// The live line: the chosen keys, or the keys that moved last.
fn live_line(
    model: &Model,
    stats: &BTreeMap<u8, KeyStats>,
    recent: &[u8],
    filter: &[KeyPos],
    full: u16,
) -> String {
    let keys: Vec<u8> = if filter.is_empty() {
        recent.iter().rev().take(LIVE_KEYS).copied().collect()
    } else {
        filter.iter().map(|p| p.index()).collect()
    };
    keys.iter()
        .map(|index| {
            let depth = stats.get(index).map_or(0, |s| s.last);
            bar(&model.key_name(*index), depth, full)
        })
        .collect::<Vec<_>>()
        .join("  ")
}

/// One depth report as a JSON line; `t_ms` is the time since the test was
/// opened when the report was drained (within one poll slice of its arrival).
fn report_json(model: &Model, report: &TravelReport, t_ms: u64) -> String {
    let value = serde_json::json!({
        "t_ms": t_ms,
        "event": "depth",
        "key": model.key_name(report.pos.index()),
        "index": report.pos.index(),
        "depth_units": report.depth,
        "depth_mm": Distance::from_units(report.depth).to_mm(),
        "variant": match report.variant {
            TravelVariant::Short => "short",
            TravelVariant::Long(_) => "long",
        },
    });
    format!("{value}\n")
}

/// Why the test stopped, in words.
fn stop_text(stop: &TravelStop, duration: Duration) -> String {
    match stop {
        TravelStop::Requested => "stopped on request (Ctrl+C)".to_string(),
        TravelStop::MaxOpen => format!(
            "the time limit ({}) was reached",
            seconds_text(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        ),
        TravelStop::ProfileChanged { profile } => format!(
            "the keyboard switched to profile {profile} (Fn + {profile}); the test was closed"
        ),
        TravelStop::OtherProgram { evidence } => {
            format!("another program talked to the keyboard ({evidence}); the test was closed")
        }
        TravelStop::KeyboardReset => "the keyboard reported a reset (Fn + Space); nothing more \
                                      was sent, not even the close"
            .to_string(),
        TravelStop::Disconnected => "the connection was lost".to_string(),
        TravelStop::SinkStopped => "the output could not be written".to_string(),
    }
}

/// What the close achieved, in words.
fn close_text(close: &CloseResult) -> String {
    match close {
        CloseResult::Acked => "The keyboard acknowledged the close: the test is off.".to_string(),
        CloseResult::NotAcked => "The close was NOT acknowledged: the keyboard may still send \
                                  key depths. Run `openwraith travel --close`."
            .to_string(),
        CloseResult::NotSent { reason } => format!(
            "No close was sent ({reason}). Reconnect the keyboard and run `openwraith travel \
             --close`."
        ),
    }
}

/// `openwraith travel [--duration 30s] [--keys W,A] [--json]` and
/// `openwraith travel --close`.
pub fn travel<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &TravelArgs,
) -> anyhow::Result<Exit> {
    ui.note(CLOSE_SITE_NOTE);
    if args.close {
        let mut session = connector.open_write()?;
        let result = close_on(&mut session, ui);
        report_session(ui, globals, &session);
        return result;
    }
    ui.enable_cancel_close_first();
    let mut session = connector.open_write()?;
    let result = travel_on(&mut session, ui, args);
    report_session(ui, globals, &session);
    result
}

fn travel_on<T: WriteTransport>(
    session: &mut Session<T>,
    ui: &mut impl Ui,
    args: &TravelArgs,
) -> anyhow::Result<Exit> {
    let summary = read_summary(session)?;
    let Some(model) = summary.model() else {
        anyhow::bail!("unsupported keyboard; nothing was sent");
    };
    let full = summary.trigger_global.max_travel.units();
    let filter = args
        .keys
        .iter()
        .map(|k| k.resolve_as(model, KeyUse::Travel).map_err(UsageError))
        .collect::<Result<Vec<_>, _>>()?;
    if ui.cancelled() {
        ui.print("Cancelled before the key-depth test was opened; nothing was sent.\n")?;
        return Ok(Exit::Failure);
    }
    ui.note(&format!(
        "Opening the key-depth test for at most {} (it stores nothing on the keyboard). Press \
         keys to see how deep they are; Ctrl+C stops, and the test is always closed again.",
        seconds_text(u64::try_from(args.duration.as_millis()).unwrap_or(u64::MAX))
    ));
    let options = TravelOptions {
        max_open: args.duration,
        ..TravelOptions::default()
    };
    let consent = WriteConsent::new(ConsentOrigin::CommandLineFlag);
    let mut stats: BTreeMap<u8, KeyStats> = BTreeMap::new();
    let mut recent: Vec<u8> = Vec::new();
    let mut shown = String::new();
    let mut output_error: Option<anyhow::Error> = None;
    let outcome = live_travel(session, consent, &options, |frame: &TravelFrame| {
        for report in &frame.reports {
            let index = report.pos.index();
            if !filter.is_empty() && !filter.contains(&report.pos) {
                continue;
            }
            let entry = stats.entry(index).or_default();
            entry.reports += 1;
            entry.last = report.depth;
            entry.peak = entry.peak.max(report.depth);
            recent.retain(|i| *i != index);
            recent.push(index);
            if args.json
                && let Err(e) = ui.print(&report_json(model, report, frame.open_ms))
            {
                output_error = Some(e);
            }
        }
        if !args.json {
            let line = live_line(model, &stats, &recent, &filter, full);
            if !line.is_empty() && line != shown {
                ui.progress(&line);
                shown = line;
            }
        }
        if output_error.is_some() || ui.cancelled() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    ui.clear_progress();
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(e) => {
            // The open went out but neither it nor a close was acknowledged
            // (a reset or a lost connection meanwhile): say how to stop it.
            if session.travel_state() == TravelState::MaybeOpen {
                ui.note(
                    "The key-depth test may still be open on the keyboard: reconnect it and run \
                     `openwraith travel --close`.",
                );
            }
            return Err(e.into());
        }
    };
    if let Some(e) = output_error {
        ui.note(&close_text(&outcome.close));
        return Err(e);
    }
    print_outcome(ui, model, args, &stats, &outcome)?;
    let clean = matches!(
        outcome.stop,
        TravelStop::Requested | TravelStop::MaxOpen | TravelStop::ProfileChanged { .. }
    );
    Ok(if clean && outcome.close == CloseResult::Acked {
        Exit::Success
    } else {
        Exit::Failure
    })
}

/// The end of a live view: why it stopped, the close, a table per key.
fn print_outcome(
    ui: &mut impl Ui,
    model: &Model,
    args: &TravelArgs,
    stats: &BTreeMap<u8, KeyStats>,
    outcome: &TravelOutcome,
) -> anyhow::Result<()> {
    if args.json {
        let value = serde_json::json!({
            "event": "stopped",
            "outcome": outcome,
            "reason": stop_text(&outcome.stop, args.duration),
        });
        return ui.print(&format!("{value}\n"));
    }
    let mut out = format!(
        "Key-depth test: {}.\n{}\n",
        stop_text(&outcome.stop, args.duration),
        close_text(&outcome.close)
    );
    if stats.is_empty() {
        out.push_str("No key moved while the test was open.\n");
    } else {
        let mut table = Table::new(&["Key", "Reports", "Deepest", "Last"]).right_align(&[1]);
        for (index, s) in stats {
            table.row(vec![
                model.key_name(*index),
                s.reports.to_string(),
                mm(s.peak),
                mm(s.last),
            ]);
        }
        out.push_str(&table.render("  "));
    }
    out.push_str(&format!(
        "{} depth report(s) in {} (slow presses update about every 72 ms; this is a view, not \
         analog input).\n",
        outcome.reports,
        seconds_text(outcome.open_ms)
    ));
    if outcome.dropped > 0 {
        out.push_str(&format!(
            "{} report(s) were dropped because they came faster than they were shown.\n",
            outcome.dropped
        ));
    }
    ui.print(&out)
}

/// `openwraith travel --close`: the close alone.
fn close_on<T: WriteTransport>(session: &mut Session<T>, ui: &mut impl Ui) -> anyhow::Result<Exit> {
    read_summary(session)?;
    ui.note("Sending the close of the key-depth test (21 .. 18 03) and nothing else...");
    let result = {
        let mut writer = session.writer(WriteConsent::new(ConsentOrigin::CommandLineFlag))?;
        writer.close_stray_travel_test()?
    };
    let stray = session.stats().stray_travel_reports;
    let mut out = match result {
        CloseResult::Acked => {
            "The keyboard acknowledged the close: no key-depth test is running now.\n".to_string()
        }
        other => format!("{}\n", close_text(&other)),
    };
    if stray > 0 {
        out.push_str(&format!(
            "Before it, {stray} key-depth report(s) arrived: a test was still open.\n"
        ));
    } else {
        out.push_str(
            "No key-depth report arrived before it (no key moved, or no test was open; the close \
             is harmless either way).\n",
        );
    }
    ui.print(&out)?;
    Ok(if result == CloseResult::Acked {
        Exit::Success
    } else {
        Exit::Failure
    })
}

// ----- monitor -----

/// `openwraith monitor [--duration 30s] [--json]`: read-only.
pub fn monitor<C: Connector, U: Ui>(
    connector: &mut C,
    ui: &mut U,
    globals: &Globals,
    args: &MonitorArgs,
) -> anyhow::Result<Exit> {
    ui.enable_cancel();
    // A read-only session: this transport cannot send a write.
    let mut session = connector.open_read()?;
    let result = monitor_on(&mut session, ui, args);
    report_session(ui, globals, &session);
    result
}

/// How long one listen lasts before the monitor looks at what came.
const MONITOR_SLICE: Duration = Duration::from_millis(100);

/// One line (or JSON object) of the monitor; `raw`: the bytes that arrived,
/// if the line is about one report.
fn monitor_line(
    ui: &mut impl Ui,
    json: bool,
    at: Duration,
    kind: &str,
    text: &str,
    raw: Option<&str>,
) -> anyhow::Result<()> {
    let ms = u64::try_from(at.as_millis()).unwrap_or(u64::MAX);
    if json {
        let mut value = serde_json::json!({"t_ms": ms, "kind": kind, "text": text});
        if let (Some(raw), Some(object)) = (raw, value.as_object_mut()) {
            object.insert("raw".to_string(), serde_json::Value::from(raw));
        }
        ui.print(&format!("{value}\n"))
    } else {
        ui.print(&format!("{:>7}.{:03} s  {text}\n", ms / 1000, ms % 1000))
    }
}

/// The bytes of a report as they arrived: the header and the declared
/// payload, and on up to the last non-zero byte (unlike
/// [`Report::summary`], nothing after the declared length is hidden).
fn raw_hex(report: &Report) -> String {
    let bytes = report.as_bytes();
    let declared = (HEADER_LEN + usize::from(report.declared_len())).min(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| *b != 0)
        .map_or(declared, |i| (i + 1).max(declared));
    HexBytes::from(&bytes[..end]).to_spaced_hex()
}

/// What an event the keyboard pushed means, with the bytes that arrived
/// (`raw`, never rebuilt from the decoded fields).
fn event_text(event: &DeviceEvent, raw: &str) -> Option<(&'static str, String)> {
    match event {
        DeviceEvent::ProfileReport { profile, .. } => Some((
            "profile_report",
            format!(
                "profile report: profile {profile} is active (raw: {raw}; Fn + {profile} on the \
                 keyboard, or another program)"
            ),
        )),
        DeviceEvent::SocdPower { power, .. } => Some((
            "socd_push",
            format!(
                "SOCD push: SOCD is now {} (raw: {raw}; Fn + Right Shift on the keyboard)",
                if *power == 1 { "on" } else { "off" }
            ),
        )),
        _ => None,
    }
}

fn monitor_on<T: Transport>(
    session: &mut Session<T>,
    ui: &mut impl Ui,
    args: &MonitorArgs,
) -> anyhow::Result<Exit> {
    ui.note(&format!(
        "Listening to the keyboard for {} (read-only: nothing is sent, not even a read). Press \
         keys such as Fn + 1..4 or Fn + Right Shift; Ctrl+C stops.",
        seconds_text(u64::try_from(args.duration.as_millis()).unwrap_or(u64::MAX))
    ));
    let start = session.observed_for();
    let mut events = session.events().len();
    let mut last: SessionStats = session.stats().clone();
    let mut reset = false;
    loop {
        let elapsed = session.observed_for().saturating_sub(start);
        if ui.cancelled() || elapsed >= args.duration {
            break;
        }
        let wait = MONITOR_SLICE.min(args.duration - elapsed);
        let heard = session.listen(wait);
        let at = session.observed_for().saturating_sub(start);
        let new_events: Vec<DeviceEvent> =
            session.events().get(events..).unwrap_or_default().to_vec();
        let new_reports: Vec<Report> = session
            .event_reports()
            .get(events..)
            .unwrap_or_default()
            .to_vec();
        events = session.events().len();
        for (event, report) in new_events.iter().zip(&new_reports) {
            let raw = raw_hex(report);
            if let Some((kind, text)) = event_text(event, &raw) {
                monitor_line(ui, args.json, at, kind, &text, Some(&raw))?;
            }
        }
        // Profile reports are also counted as ignored reports.
        let profile_reports = new_events
            .iter()
            .filter(|e| matches!(e, DeviceEvent::ProfileReport { .. }))
            .count();
        let profile_reports = u64::try_from(profile_reports).unwrap_or(u64::MAX);
        let stats = session.stats().clone();
        let delta = |now: u64, before: u64| now.saturating_sub(before);
        let lines: [(&str, u64, &str); 4] = [
            (
                "status",
                delta(stats.status_reports, last.status_reports),
                "status report(s) nobody here asked for: another program (the vendor site?) \
                 is talking to the keyboard",
            ),
            (
                "foreign_ack",
                delta(stats.foreign_acks, last.foreign_acks),
                "write acknowledgement(s) of a write OpenWraith did not send: another program \
                 is writing",
            ),
            (
                "travel_stream",
                delta(stats.stray_travel_reports, last.stray_travel_reports),
                "key-depth report(s): a key-depth test is open (stop it with `openwraith travel \
                 --close`)",
            ),
            (
                "other",
                delta(stats.ignored, last.ignored)
                    .saturating_sub(delta(stats.foreign_acks, last.foreign_acks))
                    .saturating_sub(profile_reports),
                "other report(s), e.g. replies to another program's reads",
            ),
        ];
        for (kind, count, text) in lines {
            if count > 0 {
                monitor_line(ui, args.json, at, kind, &format!("{count} {text}"), None)?;
            }
        }
        last = stats;
        match heard {
            Ok(_) => {}
            Err(ListenError::KeyboardReset { report }) => {
                monitor_line(
                    ui,
                    args.json,
                    at,
                    "reset",
                    &format!(
                        "reset notification ({report}; Fn + Space held): the keyboard reset its \
                         settings. It is never answered; stopped listening."
                    ),
                    None,
                )?;
                reset = true;
                break;
            }
            Err(e) => return Err(e.into()),
        }
    }
    let stats = session.stats();
    if !args.json {
        let heard = session.observed_for().saturating_sub(start);
        ui.print(&format!(
            "Listened for {}: {} profile report(s), {} SOCD push(es), {} key-depth report(s), {} \
             status report(s) and {} write acknowledgement(s) of other programs.\n",
            seconds_text(u64::try_from(heard.as_millis()).unwrap_or(u64::MAX)),
            session
                .events()
                .iter()
                .filter(|e| matches!(e, DeviceEvent::ProfileReport { .. }))
                .count(),
            stats.socd_pushes,
            stats.stray_travel_reports,
            stats.status_reports,
            stats.foreign_acks
        ))?;
    }
    Ok(if reset { Exit::Failure } else { Exit::Success })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwraith_protocol::W75;

    #[test]
    fn bars_scale_to_the_travel() {
        assert_eq!(bar("W", 0, 170), "W [----------] 0.00 mm");
        assert_eq!(bar("W", 85, 170), "W [#####-----] 1.70 mm");
        assert_eq!(bar("W", 170, 170), "W [##########] 3.40 mm");
        assert_eq!(bar("W", 200, 170), "W [##########] 4.00 mm");
        let mut stats = BTreeMap::new();
        stats.insert(
            46,
            KeyStats {
                reports: 1,
                peak: 34,
                last: 34,
            },
        );
        let line = live_line(&W75, &stats, &[46], &[], 170);
        assert_eq!(line, "W [##--------] 0.68 mm");
        // A chosen key that has not moved is shown at 0.
        let a = W75.key_by_name("A").and_then(|k| k.pos()).expect("A");
        let line = live_line(&W75, &stats, &[46], &[a], 170);
        assert_eq!(line, "A [----------] 0.00 mm");
    }

    /// A report from its first bytes (the rest zero).
    fn report(prefix: &[u8]) -> Report {
        let mut bytes = [0u8; openwraith_protocol::REPORT_LEN];
        bytes[..prefix.len()].copy_from_slice(prefix);
        Report::new(bytes)
    }

    #[test]
    fn pushes_are_described_with_the_bytes_that_arrived() {
        let socd = report(&[
            0x24, 0x02, 0, 0, 0x06, 0x01, 0, 0, 0, 0xfa, 0xe5, 0x00, 0x07,
        ]);
        let event = DeviceEvent::classify(&socd).expect("SOCD push");
        let (kind, text) = event_text(&event, &raw_hex(&socd)).expect("SOCD push");
        assert_eq!(kind, "socd_push");
        // Every byte up to the last non-zero one, also past byte 10.
        assert!(
            text.contains("SOCD is now on (raw: 24 02 00 00 06 01 00 00 00 fa e5 00 07;"),
            "{text}"
        );
        // A push with another length byte and a byte past the declared
        // payload: shown as it arrived, not rebuilt as `03 11 PP XX`.
        let push = report(&[0x21, 0, 0, 0, 0x02, 0x11, 0x02, 0x00, 0x5a]);
        let event = DeviceEvent::classify(&push).expect("profile report");
        let (_, text) = event_text(&event, &raw_hex(&push)).expect("profile report");
        assert!(text.contains("raw: 21 00 00 00 02 11 02 00 5a;"), "{text}");
        assert!(!text.contains("03 11"), "{text}");
        assert_eq!(raw_hex(&report(&[0x01])), "01 00 00 00 00");
        // The declared payload is shown even when it ends in zeros.
        assert_eq!(
            raw_hex(&report(&[0x21, 0, 0, 0, 0x03, 0x11, 0x02, 0x00])),
            "21 00 00 00 03 11 02 00"
        );
        assert!(event_text(&DeviceEvent::Status, "01").is_none());
    }
}
