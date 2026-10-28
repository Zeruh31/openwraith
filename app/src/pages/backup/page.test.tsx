// The Backup & Restore page's flows on the mocked IPC, replayed from the
// fixtures the core recorded on the simulator: file → profiles → compare
// (with the switch consent) → review → explicit Write → report; the
// backend asking again for consent; cancelling; errors; Undo through the
// route; a disconnected keyboard; and both kinds of backup.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../../i18n";
import type { AutoBackupsView } from "../../ipc/bindings";
import { makeFormat } from "../../i18n/format";
import { createMockBackend } from "../../preview/mockBackend";
import { FIXTURES } from "../../preview/registry";
import { appInfoStore, DEFAULT_APP_INFO } from "../../state/appInfo";
import { acknowledgeVendor, resetSession } from "../../state/session";
import { connectEvents, INITIAL_STATUS, statusStore } from "../../state/status";
import { clearToasts } from "../../state/toasts";
import BackupRestorePage from "./index";

const en = DICTIONARIES.en;
const t = en.backupRestore;
const format = makeFormat("en");
/** A little more than the confirmation buttons' arming delay. */
const ARM_MS = 560;

interface Call {
  command: string;
  args: Record<string, unknown>;
}

let calls: Call[] = [];

function named(command: string): Call[] {
  return calls.filter((call) => call.command === command);
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

/** Mounts the page on a recorded fixture (fallbacks on, at once). */
async function mount(fixtureId: string, hash = "#/backup"): Promise<void> {
  const fixture = FIXTURES[fixtureId];
  if (fixture === undefined) throw new Error(`no fixture ${fixtureId}`);
  appInfoStore.set(fixture.app_info);
  const backend = createMockBackend(fixture, { speed: 0, replayEvents: false, fallback: true, library: FIXTURES });
  mockIPC((command, args) => {
    calls.push({ command, args: (args ?? {}) as Record<string, unknown> });
    return backend.handle(command, args);
  });
  await act(async () => {
    await connectEvents();
  });
  window.location.hash = hash;
  render(<BackupRestorePage />);
}

/** Clicks a confirmation button once it is armed. */
async function confirmArmed(button: HTMLElement): Promise<void> {
  await sleep(ARM_MS);
  fireEvent.click(button);
}

async function openFile(): Promise<void> {
  fireEvent.click(screen.getByRole("button", { name: t.restore.openFile }));
  await screen.findByRole("button", { name: t.restore.compare });
}

beforeEach(() => {
  setLang("en");
  statusStore.set(INITIAL_STATUS);
  resetSession();
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

describe("restore", () => {
  it("compares after the switch consent, writes only after Write, and reports", async () => {
    await mount("restore_ok_persisted");
    await openFile();
    expect(screen.getByText("restore.owbackup.json")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: t.restore.compare }));
    const consent = await screen.findByRole("dialog", { name: t.restore.consentTitle });
    expect(within(consent).getByText(t.restore.consentBody({ list: "2, 3 and 4", count: 3, active: 1 }))).toBeTruthy();
    expect(named("prepare_restore")).toHaveLength(0);
    await confirmArmed(within(consent).getByRole("button", { name: t.restore.consentConfirm }));

    const write = await screen.findByRole("button", { name: t.review.confirm({ n: 3 }) });
    expect(named("prepare_restore").map((c) => c.args["intent"])).toEqual([
      { source: "f-084dc30f22a4672b", profiles: null, allow_firmware_mismatch: false, confirmed_switches: [2, 3, 4] },
    ]);
    // The review: per-profile changes, the Fn re-send note, what isn't restored.
    expect(screen.getByText(t.review.profileSummary({ n: 1, changes: 2 }))).toBeTruthy();
    expect(screen.getByText(en.flow.commitTrigger.base_keymap)).toBeTruthy();
    expect(screen.getByText(t.review.profileSkipped({ n: 1 }))).toBeTruthy();
    expect(named("apply_restore")).toHaveLength(0);

    await confirmArmed(write);
    fireEvent.click(write);
    await screen.findByText(t.result.headline.done({ n: 3 }));
    expect(named("apply_restore")).toHaveLength(1);
    expect(screen.getByText("openwraith-W75HEARGB-20261024-073000-restore.owbackup.json")).toBeTruthy();
    expect(screen.getByRole("button", { name: en.common.actions.undo })).toBeTruthy();
    // A new safety backup exists: the list is read again.
    await waitFor(() => {
      expect(named("list_safety_backups")).toHaveLength(2);
    });
  });

  it("asks for the vendor tick before the comparison's profile switches (first write of the run)", async () => {
    resetSession();
    await mount("restore_ok_persisted");
    await openFile();
    fireEvent.click(screen.getByRole("button", { name: t.restore.compare }));
    const consent = await screen.findByRole("dialog", { name: t.restore.consentTitle });
    const confirm = within(consent).getByRole("button", { name: t.restore.consentConfirm });
    await sleep(ARM_MS);
    expect(confirm).toHaveProperty("disabled", true);
    expect(within(consent).getByText(en.flow.review.vendorTickNeeded)).toBeTruthy();
    fireEvent.click(confirm);
    expect(named("prepare_restore")).toHaveLength(0);
    fireEvent.click(within(consent).getByLabelText(en.flow.review.vendorTick));
    expect(confirm).toHaveProperty("disabled", false);
    fireEvent.click(confirm);
    await waitFor(() => {
      expect(named("prepare_restore")).toHaveLength(1);
    });
    // The tick counts for the run: the review asks no more.
    await screen.findByRole("button", { name: t.review.confirm({ n: 3 }) });
    expect(screen.queryByLabelText(en.flow.review.vendorTick)).toBeNull();
  });

  it("asks for the switches again when the backend names other profiles", async () => {
    await mount("restore_needs_switch_consent");
    await openFile();
    fireEvent.click(screen.getByRole("button", { name: t.restore.compare }));
    const consent = await screen.findByRole("dialog", { name: t.restore.consentTitle });
    await confirmArmed(within(consent).getByRole("button", { name: t.restore.consentConfirm }));
    await waitFor(() => {
      expect(named("prepare_restore")).toHaveLength(1);
    });
    const again = await screen.findByRole("dialog", { name: t.restore.consentTitle });
    expect(within(again).getByText(t.restore.consentBody({ list: "2, 3 and 4", count: 3, active: 1 }))).toBeTruthy();
    expect(screen.queryByRole("dialog", { name: en.flow.opKinds.restore_prepare })).toBeNull();
    expect(named("apply_restore")).toHaveLength(0);
  });

  it("opens a safety backup from the route (Undo), offers the undo review and discards it on Cancel", async () => {
    await mount("undo_preview", "#/backup?restore=f-6b33354e1a23f0da");
    await screen.findByRole("button", { name: t.restore.compare });
    expect(named("open_safety_backup").map((c) => c.args["file"])).toEqual(["f-6b33354e1a23f0da"]);
    await waitFor(() => {
      expect(window.location.hash).toBe("#/backup");
    });
    expect(screen.getByText(t.safetyReasons.set_trigger)).toBeTruthy();
    // The wizard is brought into view: its first step has the focus.
    await waitFor(() => {
      expect(document.activeElement?.textContent).toContain(t.restore.step1);
    });

    // Profile 1 is active and the only one in scope: no switch, no consent.
    fireEvent.click(screen.getByRole("button", { name: t.restore.compare }));
    const title = t.review.undoTitle({ list: "1", count: 1, date: format.dateTime("2026-10-24T07:30:00Z") });
    const flow = await screen.findByRole("dialog", { name: title });
    expect(within(flow).getByText(t.review.undoNote({ list: "1", count: 1 }))).toBeTruthy();
    fireEvent.click(within(flow).getByRole("button", { name: en.common.actions.cancel }));
    await waitFor(() => {
      expect(named("discard_plan")).toHaveLength(1);
    });
    expect(named("discard_plan")[0]?.args["token"]).toBe("p-6de76e102ec5ae48");
    expect(named("apply_restore")).toHaveLength(0);
  });

  it("needs the explicit opt-in for a firmware mismatch", async () => {
    await mount("restore_firmware_mismatch");
    await openFile();
    expect(screen.getByText(t.restore.firmwareTitle)).toBeTruthy();
    const compare = screen.getByRole("button", { name: t.restore.compare });
    expect(compare).toHaveProperty("disabled", true);
    expect(screen.getByText(t.reasons.firmwareTick)).toBeTruthy();

    fireEvent.click(screen.getByLabelText(t.restore.firmwareAnyway));
    expect(compare).toHaveProperty("disabled", false);
    fireEvent.click(compare);
    await waitFor(() => {
      expect(named("prepare_restore")).toHaveLength(1);
    });
    expect(named("prepare_restore")[0]?.args["intent"]).toMatchObject({ allow_firmware_mismatch: true });
  });

  it("shows why a file can't be used", async () => {
    await mount("restore_invalid_file");
    fireEvent.click(screen.getByRole("button", { name: t.restore.openFile }));
    await screen.findByText(en.errors.kind.invalid_backup.title);
    fireEvent.click(screen.getByRole("button", { name: t.restore.openFile }));
    await screen.findByText(en.errors.detail.unsupported_schema({ found: 9, supported: 2 }));
    expect(screen.queryByRole("button", { name: t.restore.compare })).toBeNull();
  });

  it("reports LOST and offers to run the same restore again", async () => {
    await mount("restore_lost");
    await openFile();
    for (const profile of [1, 3, 4]) fireEvent.click(screen.getByLabelText(en.common.profile({ n: profile })));
    fireEvent.click(screen.getByRole("button", { name: t.restore.compare }));
    const consent = await screen.findByRole("dialog", { name: t.restore.consentTitle });
    expect(within(consent).getByText(t.restore.consentBody({ list: "2", count: 1, active: 1 }))).toBeTruthy();
    await confirmArmed(within(consent).getByRole("button", { name: t.restore.consentConfirm }));
    expect(named("prepare_restore")[0]?.args["intent"]).toMatchObject({ profiles: [2], confirmed_switches: [2] });

    const write = await screen.findByRole("button", { name: t.review.confirm({ n: 1 }) });
    await confirmArmed(write);
    await screen.findByText(t.result.headline.lost({ n: 1 }));
    expect(screen.getByText(en.flow.results.lost)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: t.result.runAgain }));
    await screen.findByRole("dialog", { name: t.restore.consentTitle });
  });

  it("reports MAY BE LOST after a disconnect and needs the keyboard to read again", async () => {
    await mount("restore_maybe_lost_disconnect");
    await openFile();
    for (const profile of [2, 3, 4]) fireEvent.click(screen.getByLabelText(en.common.profile({ n: profile })));
    fireEvent.click(screen.getByRole("button", { name: t.restore.compare }));
    const write = await screen.findByRole("button", { name: t.review.confirm({ n: 2 }) });
    expect(screen.queryByRole("dialog", { name: t.restore.consentTitle })).toBeNull();
    await confirmArmed(write);
    await screen.findByText(t.result.headline.maybeLost({ n: 2 }));
    expect(screen.getAllByText(en.flow.results.maybeLost).length).toBeGreaterThan(0);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: en.common.actions.readAgain })).toHaveProperty("disabled", true);
    });
    expect(screen.getByText(t.result.connectFirst)).toBeTruthy();
  });

  it("restores a safety backup chosen from the list", async () => {
    await mount("safety_list");
    const restore = await screen.findAllByRole("button", { name: /^Restore the safety backup from/u });
    expect(restore).toHaveLength(2);
    expect(screen.getByText(t.safetyReasons.set_light)).toBeTruthy();
    const first = restore[0];
    if (first === undefined) throw new Error("no row");
    fireEvent.click(first);
    await screen.findByRole("button", { name: t.restore.compare });
    expect(named("open_safety_backup").map((c) => c.args["file"])).toEqual(["f-084dc30f22a4672b"]);
  });
});

describe("back up", () => {
  it("backs up all profiles after the review and shows the saved file", async () => {
    await mount("backup_all_ok");
    const all = screen.getByRole("button", { name: t.backup.all });
    fireEvent.click(all);
    fireEvent.click(all);
    const write = await screen.findByRole("button", { name: t.backup.confirm });
    expect(named("choose_backup_destination")).toEqual([
      { command: "choose_backup_destination", args: { kind: "all_profiles", dialog: { title: t.dialog.saveAll, filter: t.dialog.filter } } },
    ]);
    expect(screen.getByText(t.backup.reviewSwitches({ visits: "2, 3 and 4", original: 1 }))).toBeTruthy();
    expect(named("apply_all_profile_backup")).toHaveLength(0);
    await confirmArmed(write);
    await screen.findByText(t.backup.saved);
    expect(screen.getByText(t.backup.savedSummary({ profiles: "All profiles (1–4)", firmware: "3.22.07" }))).toBeTruthy();
    expect(named("apply_all_profile_backup")).toHaveLength(1);
  });

  it("says nothing was written when the all-profile backup is stopped", async () => {
    await mount("backup_all_cancelled");
    fireEvent.click(screen.getByRole("button", { name: t.backup.all }));
    await confirmArmed(await screen.findByRole("button", { name: t.backup.confirm }));
    await screen.findByText(en.errors.kind.cancelled.title);
    expect(screen.getByText(en.flow.nothingWrittenSwitches({ n: 1 }))).toBeTruthy();
    expect(screen.getByText(t.backup.noFileSaved)).toBeTruthy();
  });

  it("backs up the active profile without a confirmation", async () => {
    await mount("backup_active_ok");
    fireEvent.click(screen.getByRole("button", { name: t.backup.active }));
    await screen.findByText(t.backup.saved);
    expect(screen.getByText(t.backup.savedSummary({ profiles: "Profile 1 only", firmware: "3.22.07" }))).toBeTruthy();
    expect(named("backup_active_profile").map((c) => c.args["destination"])).toEqual(["f-90f303b6f8f26cf4"]);
    expect(calls.some((c) => c.command.startsWith("prepare_"))).toBe(false);
  });

  it("does nothing when the save dialog is cancelled", async () => {
    appInfoStore.set(DEFAULT_APP_INFO);
    statusStore.set({ ...INITIAL_STATUS, revision: 5, connection: { state: "ready" }, active_profile: 1, write_gate: { allowed: true, reasons: [] } });
    mockIPC((command, args) => {
      calls.push({ command, args: (args ?? {}) as Record<string, unknown> });
      if (command === "list_safety_backups") return [];
      if (command === "list_auto_backups") {
        const empty: AutoBackupsView = {
          enabled: true,
          keep: 10,
          connected_keyboard: true,
          unidentified: false,
          entries: [],
        };
        return empty;
      }
      return null;
    });
    window.location.hash = "#/backup";
    render(<BackupRestorePage />);
    fireEvent.click(screen.getByRole("button", { name: t.backup.all }));
    await waitFor(() => {
      expect(named("choose_backup_destination")).toHaveLength(1);
    });
    await sleep(50);
    // Only the dialog and the two lists (file names only): nothing reaches the keyboard.
    expect(calls.map((c) => c.command).sort()).toEqual([
      "choose_backup_destination",
      "list_auto_backups",
      "list_safety_backups",
    ]);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("without a keyboard", () => {
  it("disables backing up and comparing with the reason, but opens files", async () => {
    await mount("searching");
    expect(screen.getByRole("button", { name: t.backup.all })).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", { name: t.backup.active })).toHaveProperty("disabled", true);
    expect(screen.getAllByText(t.reasons.notConnected).length).toBeGreaterThan(0);

    await openFile();
    expect(screen.getByRole("button", { name: t.restore.compare })).toHaveProperty("disabled", true);
    const deviceCommands = calls.filter(
      (c) => c.command.startsWith("prepare_") || c.command.startsWith("apply_") || c.command.startsWith("backup_"),
    );
    expect(deviceCommands).toEqual([]);
  });

  it("waits while an OpenWraith command uses the keyboard", async () => {
    await mount("ready");
    const ready = statusStore.get();
    act(() => {
      statusStore.set({
        ...ready,
        revision: ready.revision + 1,
        paused_for_cli: true,
        write_gate: { allowed: false, reasons: ["paused_for_cli"] },
      });
    });
    expect(screen.getByRole("button", { name: t.backup.all })).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", { name: t.share.export.button({ n: 1 }) })).toHaveProperty("disabled", true);
    expect(screen.getAllByText(t.reasons.pausedForCli).length).toBeGreaterThan(1);
    fireEvent.click(screen.getByRole("button", { name: t.backup.all }));
    expect(calls.some((c) => c.command.startsWith("choose_") || c.command.startsWith("prepare_"))).toBe(false);
  });

  it("compares the opened file with the keyboard again once one has been read", async () => {
    await mount("searching");
    await openFile();
    expect(named("open_safety_backup")).toEqual([]);

    const ready = FIXTURES["ready"]?.initial_status;
    if (ready === undefined) throw new Error("no ready fixture");
    act(() => {
      statusStore.set(ready);
    });
    await waitFor(() => {
      expect(named("open_safety_backup")).toHaveLength(1);
    });
    expect(String(named("open_safety_backup")[0]?.args["file"])).toMatch(/^f-/);
    expect(screen.getByText("restore.owbackup.json")).toBeTruthy();
  });
});
