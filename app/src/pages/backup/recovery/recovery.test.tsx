// The Recovery card on the mocked IPC, replayed from the fixtures the core
// recorded on the simulator: opened by the route or by a suspected reset;
// the evidence in plain words; "It's fine, keep them" (nothing sent to the
// keyboard); one profile restored from its automatic backup through the
// standard restore review, written only after Write; "Restore all profiles"
// after the switch consent (asked again when the keyboard says so);
// "Reconnect and compare" after the keyboard's own reset report; "Restore
// anyway" after a firmware update; automatic backups off; Turkish.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../../../i18n";
import { makeFormat } from "../../../i18n/format";
import type { AppError, RecoveryView, RestorePreview } from "../../../ipc/bindings";
import { createMockBackend } from "../../../preview/mockBackend";
import { FIXTURES } from "../../../preview/registry";
import { resetActions } from "../../../shell/actions";
import { appInfoStore, DEFAULT_APP_INFO } from "../../../state/appInfo";
import { acknowledgeVendor, resetSession } from "../../../state/session";
import { connectEvents, INITIAL_STATUS, statusStore } from "../../../state/status";
import { clearToasts } from "../../../state/toasts";
import BackupRestorePage from "../index";
import { groupPlan, planChanges } from "../model";

const en = DICTIONARIES.en;
const fmt = makeFormat("en");
const t = en.backupRestore;
const r = t.recovery;
/** A little more than the confirmation buttons' arming delay. */
const ARM_MS = 560;
const AUTO_FILE = "f-90f303b6f8f26cf4";

interface Call {
  command: string;
  args: Record<string, unknown>;
}

let calls: Call[] = [];

function named(command: string): Call[] {
  return calls.filter((call) => call.command === command);
}

/** Whether anything that could write to the keyboard (or compare by switching) was asked for. */
function anyDeviceWork(): boolean {
  return calls.some(
    (c) =>
      c.command.startsWith("prepare_") ||
      c.command.startsWith("apply_") ||
      c.command === "refresh" ||
      c.command === "export_profile",
  );
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

/** Answers that replace the fixture's for some commands. */
type Overrides = Partial<Record<string, (args: Record<string, unknown>) => unknown>>;

/** Mounts the page on a recorded fixture (fallbacks on, at once). */
async function mount(fixtureId: string, hash = "#/backup", overrides: Overrides = {}): Promise<void> {
  const fixture = FIXTURES[fixtureId];
  if (fixture === undefined) throw new Error(`no fixture ${fixtureId}`);
  appInfoStore.set(fixture.app_info);
  const backend = createMockBackend(fixture, { speed: 0, replayEvents: false, fallback: true, library: FIXTURES });
  mockIPC((command, args) => {
    const record = (args ?? {}) as Record<string, unknown>;
    calls.push({ command, args: record });
    const override = overrides[command];
    return override === undefined ? backend.handle(command, args) : override(record);
  });
  await act(async () => {
    await connectEvents();
  });
  window.location.hash = hash;
  render(<BackupRestorePage />);
}

/** The recorded `get_recovery` answers of a fixture. */
function recordedViews(fixtureId: string): RecoveryView[] {
  return (FIXTURES[fixtureId]?.calls ?? []).flatMap((c) =>
    c.command === "get_recovery" && c.ok !== null ? [c.ok] : [],
  );
}

function recordedPreview(fixtureId: string): RestorePreview {
  const call = FIXTURES[fixtureId]?.calls.find((c) => c.command === "prepare_recovery" && c.ok !== null);
  if (call?.command !== "prepare_recovery" || call.ok === null) throw new Error(`no recorded comparison in ${fixtureId}`);
  return call.ok;
}

/**
 * A recorded view with the keyboard's reset report standing and nothing
 * held; `unchecked`: profile 2 was not compared since the report.
 */
function reportView(unchecked: boolean): RecoveryView {
  const [view] = recordedViews("recovery_keep_current");
  if (view === undefined) throw new Error("no recorded view");
  const profiles = view.profiles.map((p) => ({
    ...p,
    state: p.state === "reset_suspected" ? ("fine" as const) : p.state,
    evidence: [],
    factory_now: [],
  }));
  return {
    ...view,
    attention: true,
    reset_reported: true,
    reset_reported_at: "2026-10-24T07:30:00Z",
    profiles: profiles.map((p) => (unchecked && p.profile === 2 ? { ...p, state: "not_checked" as const } : p)),
  };
}

/** The Recovery card. */
async function card(): Promise<HTMLElement> {
  const heading = await screen.findByRole("heading", { name: r.title });
  const section = heading.closest("section");
  if (section === null) throw new Error("no card");
  return section;
}

/** Clicks a confirmation button once it is armed. */
async function confirmArmed(button: HTMLElement): Promise<void> {
  await sleep(ARM_MS);
  fireEvent.click(button);
}

beforeEach(() => {
  setLang("en");
  statusStore.set(INITIAL_STATUS);
  resetSession();
  resetActions();
  acknowledgeVendor();
  clearToasts();
  calls = [];
  window.location.hash = "";
});

afterEach(() => {
  clearMocks();
  statusStore.set(INITIAL_STATUS);
  appInfoStore.set(DEFAULT_APP_INFO);
});

describe("the Recovery card", () => {
  it("opens from the route, says that nothing looks reset, and closes", async () => {
    await mount("recovery_nothing_wrong", "#/backup?recover=1");
    const recovery = await card();
    await within(recovery).findByText(r.fineTitle);
    expect(within(recovery).getByText(r.never)).toBeTruthy();
    // The route opened the card; the page's own route is clean again.
    expect(window.location.hash).toBe("#/backup");
    // The heading is focused, so a screen reader starts here.
    expect(document.activeElement?.textContent).toBe(r.title);
    expect(within(recovery).getByText(r.states.fine)).toBeTruthy();
    expect(within(recovery).getAllByText(r.states.no_known_good)).toHaveLength(3);
    expect(within(recovery).queryByRole("button", { name: r.keep })).toBeNull();
    // A profile with an automatic backup can still be restored from it (a review, never at once).
    expect(within(recovery).getByRole("button", { name: r.restore({ n: 1 }) })).toBeTruthy();
    expect(within(recovery).getByLabelText(r.restoreFrom({ n: 1 }))).toBeTruthy();
    expect(within(recovery).queryByRole("button", { name: r.restore({ n: 2 }) })).toBeNull();
    expect(anyDeviceWork()).toBe(false);

    fireEvent.click(within(recovery).getByRole("button", { name: en.common.actions.close }));
    expect(screen.queryByRole("heading", { name: r.title })).toBeNull();
  });

  it("explains a suspected reset in plain words and keeps the settings without writing", async () => {
    await mount("recovery_keep_current");
    const recovery = await card();
    await within(recovery).findByText(r.attentionTitle);
    expect(within(recovery).getByText(r.heldProfiles({ list: "1", count: 1 }))).toBeTruthy();
    expect(within(recovery).getByText(r.evidenceTitle)).toBeTruthy();
    expect(within(recovery).getByText(r.evidence.reset_seen)).toBeTruthy();
    expect(within(recovery).getByText(r.evidence.triggers_to_factory)).toBeTruthy();
    expect(
      within(recovery).getByText(r.factoryNow({ list: `${r.tables.triggers} and ${r.tables.main_light}` })),
    ).toBeTruthy();
    expect(within(recovery).getByText(r.changed({ list: r.tables.custom_colors1 }))).toBeTruthy();
    expect(within(recovery).getByText(r.states.reset_suspected)).toBeTruthy();
    expect(within(recovery).getByText(r.never)).toBeTruthy();
    // No reset report on this connection: nothing to reconnect.
    expect(within(recovery).queryByRole("button", { name: r.reconnect })).toBeNull();
    // While it looks reset the card stays.
    expect(within(recovery).queryByRole("button", { name: en.common.actions.close })).toBeNull();

    fireEvent.click(within(recovery).getByRole("button", { name: r.keep }));
    await within(recovery).findByText(r.kept);
    expect(named("keep_current_settings").map((c) => c.args["profiles"])).toEqual([[]]);
    expect(within(recovery).getByText(r.fineTitle)).toBeTruthy();
    expect(anyDeviceWork()).toBe(false);
    // New automatic backups exist: the list is read again.
    await waitFor(() => {
      expect(named("list_auto_backups").length).toBeGreaterThan(1);
    });
  });

  it("restores a profile from its automatic backup through the standard review, only after Write", async () => {
    await mount("recovery_restore_ok");
    const recovery = await card();
    const restore = await within(recovery).findByRole("button", { name: r.restore({ n: 1 }) });
    expect(within(recovery).getByLabelText<HTMLSelectElement>(r.restoreFrom({ n: 1 })).value).toBe(AUTO_FILE);
    fireEvent.click(restore);

    const preview = recordedPreview("recovery_restore_ok");
    const review = await screen.findByRole("dialog", { name: r.reviewTitle({ list: "1", count: 1 }) });
    // The active profile: no switch, so no consent dialog.
    expect(screen.queryByRole("dialog", { name: t.restore.consentTitle })).toBeNull();
    expect(named("prepare_recovery").map((c) => c.args["intent"])).toEqual([
      { files: [AUTO_FILE], allow_firmware_mismatch: false, confirmed_switches: [] },
    ]);
    expect(within(review).getByText(t.review.profileSummary({ n: 1, changes: planChanges(groupPlan(preview.plan)) })))
      .toBeTruthy();
    expect(named("apply_restore")).toHaveLength(0);

    const write = within(review).getByRole("button", { name: t.review.confirm({ n: planChanges(groupPlan(preview.plan)) }) });
    // Enter never writes, and Write needs its arming delay.
    fireEvent.keyDown(write, { key: "Enter" });
    fireEvent.click(write);
    expect(named("apply_restore")).toHaveLength(0);
    await confirmArmed(write);
    await screen.findByText(t.result.headline.done({ n: 3 }));
    expect(named("apply_restore").map((c) => c.args["token"])).toEqual([preview.token]);
    expect(screen.getByRole("button", { name: en.common.actions.undo })).toBeTruthy();
    // The card is read again: nothing looks reset any more.
    await within(recovery).findByText(r.fineTitle);
    await waitFor(() => {
      expect(named("list_safety_backups").length).toBeGreaterThan(1);
    });
  });

  it("asks before switching profiles for “Restore all profiles”, and again when the keyboard asks", async () => {
    await mount("recovery_restore_all_preview");
    const recovery = await card();
    const all = await within(recovery).findByRole("button", { name: r.restoreAll });
    expect(within(recovery).getByText(r.restoreAllHint({ list: "1, 2, 3 and 4" }))).toBeTruthy();
    fireEvent.click(all);

    const consent = await screen.findByRole("dialog", { name: t.restore.consentTitle });
    expect(within(consent).getByText(t.restore.consentBody({ list: "2, 3 and 4", count: 3, active: 1 }))).toBeTruthy();
    expect(named("prepare_recovery")).toHaveLength(0);
    await confirmArmed(within(consent).getByRole("button", { name: t.restore.consentConfirm }));

    // The recorded keyboard asks for the switches again (the backend is the authority).
    const again = await screen.findByRole("dialog", { name: t.restore.consentTitle });
    expect(named("prepare_recovery")).toHaveLength(1);
    await confirmArmed(within(again).getByRole("button", { name: t.restore.consentConfirm }));

    const review = await screen.findByRole("dialog", { name: r.reviewTitle({ list: "1, 2, 3 and 4", count: 4 }) });
    const intents = named("prepare_recovery").map((c) => c.args["intent"]);
    const files = ["f-6f104f2bea9cf264", "f-869705ec6aa666a7", "f-3aa8b47910d062db", "f-6f2d909f705d2580"];
    expect(intents).toEqual([
      { files, allow_firmware_mismatch: false, confirmed_switches: [2, 3, 4] },
      { files, allow_firmware_mismatch: false, confirmed_switches: [2, 3, 4] },
    ]);
    expect(within(review).getByText(t.review.switched({ list: "2, 3 and 4", count: 3 }))).toBeTruthy();
    expect(named("apply_restore")).toHaveLength(0);
  });

  it("needs the vendor-site tick before the switches of “Restore all profiles”", async () => {
    resetSession();
    await mount("recovery_restore_all_preview");
    const recovery = await card();
    fireEvent.click(await within(recovery).findByRole("button", { name: r.restoreAll }));
    const consent = await screen.findByRole("dialog", { name: t.restore.consentTitle });
    const confirm = within(consent).getByRole("button", { name: t.restore.consentConfirm });
    await sleep(ARM_MS);
    expect(confirm).toHaveProperty("disabled", true);
    fireEvent.click(confirm);
    expect(named("prepare_recovery")).toHaveLength(0);
  });

  it("offers “Reconnect and compare” after the keyboard's own reset report, and sends nothing else", async () => {
    const [view] = recordedViews("recovery_reset_suspected");
    await mount("recovery_reset_suspected", "#/backup", { get_recovery: () => view });
    const recovery = await card();
    await within(recovery).findByText(r.attentionTitle);
    expect(within(recovery).getByText(r.resetReported({ time: fmt.dateTimeSeconds("2026-10-24T07:30:00Z") }))).toBeTruthy();
    expect(within(recovery).getByText(r.reconnectHint)).toBeTruthy();
    // Restoring waits for the reconnection (the keyboard reset itself).
    expect(within(recovery).getByRole("button", { name: r.restore({ n: 1 }) })).toHaveProperty("disabled", true);
    expect(within(recovery).getAllByText(t.reasons.resetSeen).length).toBeGreaterThan(0);

    fireEvent.click(within(recovery).getByRole("button", { name: r.reconnect }));
    await waitFor(() => {
      expect(named("reconnect")).toHaveLength(1);
    });
    expect(anyDeviceWork()).toBe(false);
    expect(named("keep_current_settings")).toHaveLength(0);
  });

  it("offers “It's fine, keep them” only after the comparison", async () => {
    await mount("recovery_reset_seen", "#/backup?recover=1");
    const recovery = await card();
    await within(recovery).findByText(r.attentionTitle);
    expect(within(recovery).getByRole("button", { name: r.keep })).toHaveProperty("disabled", true);
    expect(within(recovery).getByText(r.keepFirst)).toBeTruthy();
  });

  it("keeps “It's fine, keep them” off while a profile was not compared since the reset report", async () => {
    const waiting = reportView(true);
    await mount("recovery_keep_current", "#/backup", { get_recovery: () => waiting });
    const recovery = await card();
    await within(recovery).findByText(r.attentionTitle);
    expect(within(recovery).getByRole("button", { name: r.keep })).toHaveProperty("disabled", true);
    expect(within(recovery).getByText(r.keepCompareFirst)).toBeTruthy();
    expect(within(recovery).queryByText(r.keepFirst)).toBeNull();
  });

  it("lets “It's fine, keep them” acknowledge a reset report whose comparisons all came out fine", async () => {
    const done = reportView(false);
    await mount("recovery_keep_current", "#/backup", { get_recovery: () => done });
    const recovery = await card();
    await within(recovery).findByText(r.attentionTitle);
    const keep = within(recovery).getByRole("button", { name: r.keep });
    expect(keep).toHaveProperty("disabled", false);
    expect(within(recovery).queryByText(r.keepCompareFirst)).toBeNull();
    fireEvent.click(keep);
    await waitFor(() => {
      expect(named("keep_current_settings").map((c) => c.args["profiles"])).toEqual([[]]);
    });
    expect(anyDeviceWork()).toBe(false);
  });

  it("makes “Restore all profiles” the main action while profiles were not compared since the reset", async () => {
    await mount("recovery_restore_all_preview");
    const recovery = await card();
    const all = await within(recovery).findByRole("button", { name: r.restoreAll });
    expect(all.className).toContain("ow-button--primary");
    expect(within(recovery).getAllByText(r.states.not_checked).length).toBe(3);
  });

  it("says when the keyboard reports no serial number", async () => {
    const [view] = recordedViews("recovery_reset_suspected");
    if (view === undefined) throw new Error("no recorded view");
    await mount("recovery_reset_suspected", "#/backup", { get_recovery: () => ({ ...view, unidentified: true }) });
    const recovery = await card();
    await within(recovery).findByText(r.unidentified);
  });

  it("needs “Restore anyway” after a firmware update", async () => {
    // A notification opens the card (the keyboard is read again on the way).
    await mount("recovery_firmware_changed", "#/backup?recover=1");
    const recovery = await card();
    await within(recovery).findByText(r.firmwareChanged({ known: "3.22.07", current: "3.23.01" }));
    expect(within(recovery).getByText(r.firmwareTitle)).toBeTruthy();
    expect(
      within(recovery).getByText(r.evidence.firmware_changed({ known: "3.22.07", current: "3.23.01" })),
    ).toBeTruthy();
    const restore = within(recovery).getByRole("button", { name: r.restore({ n: 1 }) });
    await waitFor(() => {
      expect(within(recovery).getAllByText(t.reasons.firmwareTick).length).toBeGreaterThan(0);
    });
    expect(restore).toHaveProperty("disabled", true);

    fireEvent.click(within(recovery).getByLabelText(t.restore.firmwareAnyway));
    expect(restore).toHaveProperty("disabled", false);
    fireEvent.click(restore);
    await waitFor(() => {
      expect(named("prepare_recovery")).toHaveLength(1);
    });
    expect(named("prepare_recovery")[0]?.args["intent"]).toMatchObject({
      files: [AUTO_FILE],
      allow_firmware_mismatch: true,
    });
  });

  it("shows “Restore anyway” when the backend refuses another firmware", async () => {
    const mismatch: AppError = {
      kind: "firmware_mismatch",
      detail: { detail: "firmware_mismatch", backup: "3.10.01", keyboard: "3.22.07" },
      message: "the backup was made with other firmware",
    };
    await mount("recovery_restore_ok", "#/backup", {
      prepare_recovery: () => {
        throw mismatch;
      },
    });
    const recovery = await card();
    fireEvent.click(await within(recovery).findByRole("button", { name: r.restore({ n: 1 }) }));
    await screen.findByText(en.errors.kind.firmware_mismatch.title);
    expect(screen.getByText(en.flow.nothingWritten)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.close }));
    expect(
      within(recovery).getByText(r.firmwareBody({ backup: "3.10.01", keyboard: "3.22.07" })),
    ).toBeTruthy();
    expect(within(recovery).getByRole("button", { name: r.restore({ n: 1 }) })).toHaveProperty("disabled", true);
  });

  it("says when automatic backups are off", async () => {
    const [view] = recordedViews("recovery_nothing_wrong");
    if (view === undefined) throw new Error("no view");
    await mount("recovery_nothing_wrong", "#/backup?recover=1", {
      get_recovery: () => ({ ...view, auto_backup_enabled: false }),
    });
    const recovery = await card();
    await within(recovery).findByText(r.autoOff);
    fireEvent.click(within(recovery).getByRole("button", { name: r.openSettings }));
    expect(window.location.hash).toBe("#/settings?tab=background");
  });

  it("says when the card can't be read, and still that nothing is restored without asking", async () => {
    const failure: AppError = { kind: "internal", detail: null, message: "store unreadable" };
    await mount("recovery_nothing_wrong", "#/backup?recover=1", {
      get_recovery: () => {
        throw failure;
      },
    });
    const recovery = await card();
    await within(recovery).findByText(r.loadFailed);
    expect(within(recovery).getByText(r.never)).toBeTruthy();
  });

  it("is in Turkish", async () => {
    await mount("recovery_keep_current");
    act(() => {
      setLang("tr");
    });
    const tr = DICTIONARIES.tr.backupRestore.recovery;
    const heading = await screen.findByRole("heading", { name: tr.title });
    const recovery = heading.closest("section");
    if (recovery === null) throw new Error("no card");
    await within(recovery).findByText(tr.attentionTitle);
    expect(within(recovery).getByText(tr.heldProfiles({ list: "1", count: 1 }))).toBeTruthy();
    expect(within(recovery).getByText(tr.never)).toBeTruthy();
    expect(within(recovery).getByRole("button", { name: tr.keep })).toBeTruthy();
    expect(within(recovery).getByRole("button", { name: tr.restore({ n: 1 }) })).toBeTruthy();
  });
});
