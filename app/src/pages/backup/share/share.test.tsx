// "Share a profile" on the mocked IPC, replayed from the fixtures the core
// recorded on the simulator: export (the active profile as a pure read;
// another profile after the switch consent, asked again when the keyboard
// says so), import (the file's summary with macros as steps → target profile
// and parts → switch consent → the restore review in import words → explicit
// Write → the import report), cancelling, the replaced Fn shortcut's own
// tick, refusals and errors, and a disconnected keyboard.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../../../i18n";
import type { AppError, ImportPreview, ImportSummaryView, RestoreReportView } from "../../../ipc/bindings";
import { createMockBackend } from "../../../preview/mockBackend";
import { FIXTURES } from "../../../preview/registry";
import { appInfoStore, DEFAULT_APP_INFO } from "../../../state/appInfo";
import { acknowledgeVendor, resetSession } from "../../../state/session";
import { connectEvents, INITIAL_STATUS, statusStore } from "../../../state/status";
import { clearToasts } from "../../../state/toasts";
import BackupRestorePage from "../index";
import { groupPlan, planChanges, tallyReport } from "../model";
import { ImportReview } from "./ImportFlow";

const en = DICTIONARIES.en;
const t = en.backupRestore;
const s = t.share;
/** A little more than the confirmation buttons' arming delay. */
const ARM_MS = 560;
const FILE = "f-084dc30f22a4672b";
const PLAN = "p-77fdc8151cb50c52";

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

/** Answers that replace the fixture's for some commands. */
type Overrides = Partial<Record<string, (args: Record<string, unknown>) => unknown>>;

/** Mounts the page on a recorded fixture (fallbacks on, at once). */
async function mount(fixtureId: string, overrides: Overrides = {}): Promise<void> {
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
  window.location.hash = "#/backup";
  render(<BackupRestorePage />);
}

/** Clicks a confirmation button once it is armed. */
async function confirmArmed(button: HTMLElement): Promise<void> {
  await sleep(ARM_MS);
  fireEvent.click(button);
}

function recordedSummary(): ImportSummaryView {
  const call = FIXTURES["import_summary"]?.calls.find((c) => c.command === "open_import_file");
  if (call?.command !== "open_import_file" || call.ok === null) throw new Error("no recorded summary");
  return call.ok;
}

function recordedPreview(): ImportPreview {
  const call = FIXTURES["import_apply_ok"]?.calls.find((c) => c.command === "prepare_import");
  if (call?.command !== "prepare_import" || call.ok === null) throw new Error("no recorded import preview");
  return call.ok;
}

function recordedReport(): RestoreReportView {
  const call = FIXTURES["import_apply_ok"]?.calls.find((c) => c.command === "apply_import");
  if (call?.command !== "apply_import" || call.ok === null) throw new Error("no recorded import report");
  return call.ok;
}

async function openImport(): Promise<void> {
  fireEvent.click(screen.getByRole("button", { name: s.import.open }));
  await screen.findByRole("button", { name: s.import.review });
}

/** A part's checkbox of the import (the export has its own). */
function importBox(label: string): HTMLInputElement {
  const panel = screen.getByRole("region", { name: s.import.title });
  return within(panel).getByLabelText<HTMLInputElement>(label);
}

function chooseInto(profile: number): void {
  fireEvent.change(screen.getByLabelText(s.import.into), { target: { value: String(profile) } });
}

/** Clicks "Compare and review…", agrees to the switch to `into` and waits for the review. */
async function reviewWithSwitch(into: number): Promise<HTMLElement> {
  fireEvent.click(screen.getByRole("button", { name: s.import.review }));
  const consent = await screen.findByRole("dialog", { name: t.restore.consentTitle });
  expect(within(consent).getByText(t.restore.consentBody({ list: String(into), count: 1, active: 1 }))).toBeTruthy();
  expect(named("prepare_import")).toHaveLength(0);
  await confirmArmed(within(consent).getByRole("button", { name: t.restore.consentConfirm }));
  return screen.findByRole("dialog", { name: s.review.title({ from: 1, into }) });
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

describe("export", () => {
  it("exports the active profile as a pure read and shows what the file holds", async () => {
    await mount("export_done");
    fireEvent.change(screen.getByLabelText(s.export.name), { target: { value: "  Tournament " } });
    fireEvent.click(screen.getByRole("button", { name: s.export.button({ n: 1 }) }));
    await screen.findByText(s.export.saved);

    expect(named("choose_export_destination")).toEqual([
      {
        command: "choose_export_destination",
        args: { profile: 1, dialog: { title: s.dialog.save, filter: s.dialog.saveFilter } },
      },
    ]);
    expect(named("export_profile").map((c) => c.args["intent"])).toEqual([
      {
        source: { source: "keyboard" },
        profile: 1,
        label: "Tournament",
        destination: "f-90f303b6f8f26cf4",
        confirmed_switch: false,
        // Design B4: everything but the dead band and the SOCD key pairs.
        include: { triggers: true, lighting: true, keys: true, dead_band: false, socd: false },
      },
    ]);
    // No switch: no consent dialog, no plan, nothing written.
    expect(screen.queryByRole("dialog", { name: s.export.consentTitle })).toBeNull();
    expect(calls.some((c) => c.command.startsWith("prepare_") || c.command.startsWith("apply_"))).toBe(false);

    const result = screen.getByRole("dialog", { name: en.flow.opKinds.export });
    expect(within(result).getByText(s.export.savedSummary({ n: 1, firmware: "3.22.07" }))).toBeTruthy();
    expect(within(result).getByText("profile-1.owprofile.json")).toBeTruthy();
    expect(within(result).getByText(s.parts.socd)).toBeTruthy();
    expect(within(result).getByText(s.noSerial)).toBeTruthy();
    expect(within(result).queryByText(s.export.switched({ n: 1 }))).toBeNull();
  });

  it("exports only the ticked parts: dead band and SOCD start unticked, none ticked can't start", async () => {
    await mount("export_options");
    const checked = (label: string) => screen.getByLabelText<HTMLInputElement>(label).checked;
    const panel = screen.getByRole("region", { name: s.export.title });
    const box = (label: string) => within(panel).getByLabelText<HTMLInputElement>(label);
    expect(box(s.parts.triggers).checked).toBe(true);
    expect(box(s.parts.lighting).checked).toBe(true);
    expect(box(s.parts.keys).checked).toBe(true);
    expect(box(s.parts.dead_band).checked).toBe(false);
    expect(box(s.parts.socd).checked).toBe(false);
    expect(checked(s.export.sources.keyboard)).toBe(true);
    expect(within(panel).getByText(s.export.macrosNote)).toBeTruthy();

    // Nothing ticked: the export can't start, and says why.
    for (const part of ["triggers", "lighting", "keys"] as const) fireEvent.click(box(s.parts[part]));
    const button = screen.getByRole("button", { name: s.export.button({ n: 1 }) });
    expect(button).toHaveProperty("disabled", true);
    expect(within(panel).getByText(s.export.reasons.noPart)).toBeTruthy();
    expect(within(panel).queryByText(s.export.macrosNote)).toBeNull();
    fireEvent.click(button);
    expect(named("choose_export_destination")).toHaveLength(0);

    // Actuation, lighting and the SOCD key pairs.
    fireEvent.click(box(s.parts.triggers));
    fireEvent.click(box(s.parts.lighting));
    fireEvent.click(box(s.parts.socd));
    fireEvent.change(screen.getByLabelText(s.export.name), { target: { value: "Tournament" } });
    fireEvent.click(button);
    await screen.findByText(s.export.saved);
    expect(named("export_profile").map((c) => c.args["intent"])).toEqual([
      {
        source: { source: "keyboard" },
        profile: 1,
        label: "Tournament",
        destination: "f-90f303b6f8f26cf4",
        confirmed_switch: false,
        include: { triggers: true, lighting: true, keys: false, dead_band: false, socd: true },
      },
    ]);
    // The recorded file holds what was exported (no dead band in it).
    const result = screen.getByRole("dialog", { name: en.flow.opKinds.export });
    expect(within(result).getByText(s.contents.missing({ list: `${s.parts.dead_band}; ${s.parts.socd}` }))).toBeTruthy();
  });

  it("exports from an automatic backup without the keyboard and without a switch", async () => {
    await mount("export_from_auto_backup");
    fireEvent.click(screen.getByLabelText(s.export.sources.automatic));
    const select = await screen.findByLabelText<HTMLSelectElement>(s.export.automatic);
    expect(named("list_auto_backups").length).toBeGreaterThan(0);
    expect(select.value).toBe("f-90f303b6f8f26cf4");
    expect(within(select).getByText(/^Profile 1 · /u)).toBeTruthy();
    expect(screen.getByText(s.export.sourceHints.automatic)).toBeTruthy();
    // The keyboard's profile choice is gone: the backup holds profile 1.
    expect(screen.queryByLabelText(s.export.profile)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: s.export.button({ n: 1 }) }));
    await screen.findByText(s.export.saved);
    expect(named("choose_export_destination")[0]?.args["profile"]).toBe(1);
    expect(named("export_profile").map((c) => c.args["intent"])).toEqual([
      {
        source: { source: "file", file: "f-90f303b6f8f26cf4" },
        profile: 1,
        label: null,
        destination: "f-6b33354e1a23f0da",
        confirmed_switch: false,
        include: { triggers: true, lighting: true, keys: true, dead_band: false, socd: false },
      },
    ]);
    expect(screen.queryByRole("dialog", { name: s.export.consentTitle })).toBeNull();
    expect(calls.some((c) => c.command.startsWith("prepare_") || c.command.startsWith("apply_"))).toBe(false);
  });

  it("exports another profile of a backup file without switching profiles", async () => {
    const summary = FIXTURES["restore_ok_persisted"]?.calls.find((c) => c.command === "open_backup_file");
    if (summary?.command !== "open_backup_file" || summary.ok === null) throw new Error("no backup summary");
    await mount("export_done", { open_backup_file: () => summary.ok });
    fireEvent.click(screen.getByLabelText(s.export.sources.file));
    const button = screen.getByRole("button", { name: s.export.button({ n: 1 }) });
    expect(button).toHaveProperty("disabled", true);
    expect(screen.getByText(s.export.reasons.noFile)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: s.export.chooseFile }));
    const profile = await screen.findByLabelText<HTMLSelectElement>(s.export.fileProfile);
    expect(named("open_backup_file")[0]?.args["dialog"]).toEqual({
      title: s.dialog.openBackup,
      filter: t.dialog.filter,
    });
    expect(screen.getByText(summary.ok.file.file_name)).toBeTruthy();
    fireEvent.change(profile, { target: { value: "3" } });
    // Another profile of the file: no switch note, no consent (the keyboard isn't used).
    expect(screen.queryByText(s.export.switchNote({ n: 3 }))).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: s.export.button({ n: 3 }) }));
    await screen.findByText(s.export.saved);
    expect(named("choose_export_destination")[0]?.args["profile"]).toBe(3);
    expect(named("export_profile")[0]?.args["intent"]).toMatchObject({
      source: { source: "file", file: summary.ok.file.token },
      profile: 3,
      confirmed_switch: false,
    });
    expect(screen.queryByRole("dialog", { name: s.export.consentTitle })).toBeNull();
  });

  it("asks before switching to read another profile, and again when the keyboard asks", async () => {
    await mount("export_needs_switch");
    fireEvent.change(screen.getByLabelText(s.export.profile), { target: { value: "3" } });
    expect(screen.getByText(s.export.switchNote({ n: 3 }))).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: s.export.button({ n: 3 }) }));

    const consent = await screen.findByRole("dialog", { name: s.export.consentTitle });
    expect(within(consent).getByText(s.export.consentBody({ n: 3, active: 1 }))).toBeTruthy();
    expect(named("choose_export_destination")[0]?.args["profile"]).toBe(3);
    expect(named("export_profile")).toHaveLength(0);
    await confirmArmed(within(consent).getByRole("button", { name: s.export.consentConfirm }));

    // The recorded keyboard asks for the consent again (it is on another profile than shown).
    const again = await screen.findByRole("dialog", { name: s.export.consentTitle });
    expect(named("export_profile")).toHaveLength(1);
    await confirmArmed(within(again).getByRole("button", { name: s.export.consentConfirm }));
    await screen.findByText(s.export.saved);
    expect(screen.getByText(s.export.switched({ n: 3 }))).toBeTruthy();
    expect(named("export_profile").map((c) => c.args["intent"])).toEqual([
      expect.objectContaining({ profile: 3, confirmed_switch: true, label: null }),
      expect.objectContaining({ profile: 3, confirmed_switch: true, label: null }),
    ]);
    // One file was chosen for both tries.
    expect(named("choose_export_destination")).toHaveLength(1);
  });

  it("needs the vendor-site tick before the switch (first write of the run)", async () => {
    resetSession();
    await mount("export_needs_switch");
    fireEvent.change(screen.getByLabelText(s.export.profile), { target: { value: "3" } });
    fireEvent.click(screen.getByRole("button", { name: s.export.button({ n: 3 }) }));
    const consent = await screen.findByRole("dialog", { name: s.export.consentTitle });
    const confirm = within(consent).getByRole("button", { name: s.export.consentConfirm });
    await sleep(ARM_MS);
    expect(confirm).toHaveProperty("disabled", true);
    fireEvent.click(confirm);
    expect(named("export_profile")).toHaveLength(0);
    fireEvent.click(within(consent).getByLabelText(en.flow.review.vendorTick));
    fireEvent.click(confirm);
    await waitFor(() => {
      expect(named("export_profile")).toHaveLength(1);
    });
  });

  it("does nothing when the save dialog is cancelled", async () => {
    await mount("export_done", { choose_export_destination: () => null });
    fireEvent.click(screen.getByRole("button", { name: s.export.button({ n: 1 }) }));
    await waitFor(() => {
      expect(named("choose_export_destination")).toHaveLength(1);
    });
    await sleep(50);
    expect(named("export_profile")).toHaveLength(0);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("says that no file was saved when the export fails", async () => {
    const failure: AppError = { kind: "io", detail: { detail: "io", file_name: "profile-1.owprofile.json" }, message: "disk full" };
    await mount("export_done", {
      export_profile: () => {
        throw failure;
      },
    });
    fireEvent.click(screen.getByRole("button", { name: s.export.button({ n: 1 }) }));
    await screen.findByText(s.export.noFileSaved);
    expect(screen.getByText(en.errors.kind.io.title)).toBeTruthy();
  });
});

describe("import", () => {
  it("shows what the file holds, macros as the keys they press, with actuation and lighting chosen", async () => {
    await mount("import_summary");
    await openImport();
    expect(named("open_import_file")).toEqual([
      { command: "open_import_file", args: { dialog: { title: s.dialog.open, filter: s.dialog.openFilter } } },
    ]);
    expect(screen.getByText("shared.owprofile.json")).toBeTruthy();
    expect(screen.getByText(s.contents.titleOf({ n: 1 }))).toBeTruthy();
    expect(screen.getByText(s.contents.macro({ n: 2, steps: 2, play: en.keys.playModes.once }))).toBeTruthy();
    expect(screen.getByText("Press A, then wait 20 ms")).toBeTruthy();
    expect(screen.getByText("Release A")).toBeTruthy();

    const checked = (label: string) => importBox(label).checked;
    expect(checked(s.parts.triggers)).toBe(true);
    expect(checked(s.parts.lighting)).toBe(true);
    expect(checked(s.parts.keys)).toBe(false);
    expect(checked(s.parts.dead_band)).toBe(false);
    expect(checked(s.parts.socd)).toBe(false);
    expect(screen.getByLabelText<HTMLSelectElement>(s.import.into).value).toBe("1");
    expect(calls.some((c) => c.command.startsWith("prepare_") || c.command.startsWith("apply_"))).toBe(false);

    // Without a part there is nothing to compare.
    fireEvent.click(importBox(s.parts.triggers));
    fireEvent.click(importBox(s.parts.lighting));
    expect(screen.getByRole("button", { name: s.import.review })).toHaveProperty("disabled", true);
    expect(screen.getByText(s.reasons.noPart)).toBeTruthy();
  });

  it("compares after the switch consent, reviews in import words and discards the plan on Cancel", async () => {
    await mount("import_preview");
    await openImport();
    chooseInto(2);
    expect(screen.getByText(s.import.intoHint({ n: 2 }))).toBeTruthy();
    const review = await reviewWithSwitch(2);

    expect(named("prepare_import").map((c) => c.args["intent"])).toEqual([
      {
        source: FILE,
        from: null,
        into: 2,
        include: { triggers: true, lighting: true, keys: false, dead_band: false, socd: false },
        allowed_critical_keys: [],
        allow_firmware_mismatch: false,
        confirmed_switches: [2],
      },
    ]);
    expect(within(review).getByText(s.review.chosen({ list: `${s.parts.triggers}; ${s.parts.lighting}` }))).toBeTruthy();
    expect(within(review).getByText(t.planWarnings.imported({ from: 1, into: 2 }))).toBeTruthy();
    expect(within(review).getByText(s.review.notImported)).toBeTruthy();
    expect(within(review).getAllByText(t.skipReasons.not_imported).length).toBeGreaterThan(0);
    expect(within(review).getByText(s.review.safetyNote({ n: 2, dir: "C:\\Users\\you\\AppData\\Local\\OpenWraith\\safety-backups" }))).toBeTruthy();
    // No Fn shortcut is replaced: no danger confirmation.
    expect(within(review).queryByText(s.critical.title)).toBeNull();

    fireEvent.click(within(review).getByRole("button", { name: en.common.actions.cancel }));
    await waitFor(() => {
      expect(named("discard_plan").map((c) => c.args["token"])).toEqual([PLAN]);
    });
    expect(named("apply_import")).toHaveLength(0);
  });

  it("writes only after Write and reports the import, with Undo and a new safety backup", async () => {
    await mount("import_apply_ok");
    await openImport();
    chooseInto(2);
    fireEvent.click(importBox(s.parts.keys));
    const review = await reviewWithSwitch(2);
    expect(named("prepare_import")[0]?.args["intent"]).toMatchObject({
      include: { triggers: true, lighting: true, keys: true, dead_band: false, socd: false },
      confirmed_switches: [2],
    });
    // The Fn layer is sent again unchanged so that the keyboard keeps the key assignments.
    expect(within(review).getByText(en.flow.commitTrigger.base_keymap)).toBeTruthy();
    expect(within(review).getByText(t.planWarnings.imported_macros({ slots: "2" }))).toBeTruthy();

    const preview = recordedPreview();
    const write = within(review).getByRole("button", { name: s.review.confirm({ n: planChanges(groupPlan(preview.plan)) }) });
    expect(named("apply_import")).toHaveLength(0);
    await confirmArmed(write);

    const report = recordedReport();
    await screen.findByText(s.result.headline.done({ n: tallyReport(report).total }));
    expect(named("apply_import").map((c) => c.args["token"])).toEqual([PLAN]);
    expect(screen.getByText("openwraith-W75HEARGB-20261024-073000-import.owbackup.json")).toBeTruthy();
    expect(screen.getByRole("button", { name: en.common.actions.undo })).toBeTruthy();
    expect(screen.getByText(t.result.counts.resent)).toBeTruthy();
    // A new safety backup exists: the list is read again.
    await waitFor(() => {
      expect(named("list_safety_backups")).toHaveLength(2);
    });
  });

  it("keeps the keys out until the replaced Fn shortcuts get their tick, naming every one before and after", async () => {
    await mount("import_refused_critical");
    await openImport();
    chooseInto(2);
    fireEvent.click(importBox(s.parts.keys));
    const review = await reviewWithSwitch(2);

    // Both replaced shortcuts have their own danger note: Fn + 2 (here the target profile itself) and the reset.
    expect(within(review).getAllByText(s.critical.title)).toHaveLength(2);
    expect(within(review).getByText(s.critical.profile({ k: 2, n: 2 }))).toBeTruthy();
    expect(within(review).getByText(s.critical.reset)).toBeTruthy();
    expect(s.critical.reset).toContain("has not been tested");
    expect(within(review).getByText(s.critical.notImported)).toBeTruthy();
    expect(within(review).getAllByText(/can only be imported together/u).length).toBeGreaterThan(0);
    const label = s.critical.confirmAll({ keys: "Fn + 2 and Fn + Space", n: 2 });
    const tick = within(review).getByLabelText<HTMLInputElement>(label);
    expect(tick.checked).toBe(false);
    expect(named("prepare_import")[0]?.args["intent"]).toMatchObject({ allowed_critical_keys: [] });

    // The tick prepares a new plan that names exactly these keys (the old plan is discarded).
    fireEvent.click(tick);
    await waitFor(() => {
      expect(named("prepare_import")).toHaveLength(2);
    });
    expect(named("prepare_import")[1]?.args["intent"]).toMatchObject({
      allowed_critical_keys: [24, 116],
      include: { keys: true },
      confirmed_switches: [2],
    });
    expect(named("discard_plan").map((c) => c.args["token"])).toEqual([PLAN]);
    await waitFor(() => {
      expect(screen.getByLabelText<HTMLInputElement>(label).checked).toBe(true);
    });
    // After the tick, both notes stay, the reset still says it is untested, and the plan's own
    // warnings name the keys in red instead of "key number N".
    const after = screen.getByRole("dialog");
    expect(within(after).getAllByText(s.critical.title)).toHaveLength(2);
    expect(within(after).getByText(s.critical.reset)).toBeTruthy();
    expect(within(after).queryByText(s.critical.notImported)).toBeNull();
    expect(within(after).getByText(t.planWarnings.fn_builtin_overridden({ key: "Space", kind: "reset" }))).toBeTruthy();
    expect(within(after).getByText(t.planWarnings.fn_builtin_overridden({ key: "2", kind: "profile" }))).toBeTruthy();
    expect(within(after).queryByText(/key number/u)).toBeNull();
    expect(named("apply_import")).toHaveLength(0);
  });

  it("names every replaced shortcut by name in the “Not imported” row", async () => {
    await mount("import_refused_critical");
    await openImport();
    chooseInto(2);
    fireEvent.click(importBox(s.parts.keys));
    const review = await reviewWithSwitch(2);
    const cause = t.blockCauses.critical_fn_override({ keys: "Fn + 2 and Fn + Space", count: 2 });
    expect(within(review).getAllByText(s.review.keymapGroupBlocked({ cause })).length).toBeGreaterThan(0);
    // Not "key number 24": the keys are named, every one of them.
    expect(within(review).queryByText(/key number/u)).toBeNull();
  });

  it("says the keys stay out, without a shortcut tick, when something else in the file blocks them", async () => {
    // The file replaces Fn + 2 and Fn + Space, but its base W holds an entry OpenWraith doesn't write.
    await mount("import_keys_blocked_other");
    await openImport();
    chooseInto(2);
    fireEvent.click(importBox(s.parts.keys));
    const review = await reviewWithSwitch(2);

    expect(within(review).getByText(s.review.blocked)).toBeTruthy();
    expect(within(review).getAllByText(s.review.notImported).length).toBeGreaterThan(0);
    // No shortcut is replaced, so nothing to allow: no danger note, no tick.
    expect(within(review).queryByText(s.critical.title)).toBeNull();
    expect(within(review).queryByText(s.critical.notImported)).toBeNull();
    expect(within(review).queryByText(/^Import it anyway/u)).toBeNull();
    expect(review.querySelector(".ow-callout--danger")).toBeNull();
  });

  it("drops the shortcut tick when the allowed comparison finds the keys blocked by something else", async () => {
    const recorded = (id: string, index: number): ImportPreview => {
      const call = FIXTURES[id]?.calls.filter((c) => c.command === "prepare_import")[index];
      if (call?.command !== "prepare_import" || call.ok === null) throw new Error(`no preview in ${id}`);
      return call.ok;
    };
    // The library stops at the first cause: the replaced shortcuts first, then (allowed) another entry.
    const answers = [recorded("import_refused_critical", 0), recorded("import_keys_blocked_other", 0)];
    await mount("import_refused_critical", {
      prepare_import: () => answers[Math.min(named("prepare_import").length - 1, answers.length - 1)],
    });
    await openImport();
    chooseInto(2);
    fireEvent.click(importBox(s.parts.keys));
    const review = await reviewWithSwitch(2);
    const label = s.critical.confirmAll({ keys: "Fn + 2 and Fn + Space", n: 2 });
    fireEvent.click(within(review).getByLabelText(label));
    await waitFor(() => {
      expect(named("prepare_import")).toHaveLength(2);
    });
    expect(named("prepare_import")[1]?.args["intent"]).toMatchObject({ allowed_critical_keys: [24, 116] });
    await screen.findByText(s.review.blocked);
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).queryByLabelText(label)).toBeNull();
    expect(within(dialog).queryByText(s.critical.title)).toBeNull();
  });

  it("keeps the general note beside a replaced shortcut when another chosen part stays out", () => {
    const recorded = FIXTURES["import_refused_critical"]?.calls.filter((c) => c.command === "prepare_import")[1];
    if (recorded?.command !== "prepare_import" || recorded.ok === null) throw new Error("no allowed preview");
    // The shortcuts are allowed, and (say) the SOCD pairs aren't in the vendor site's form.
    const preview: ImportPreview = { ...recorded.ok, category_blocked: true, other_cause_blocked: true };
    const { container } = render(<ImportReview preview={preview} />);
    expect(within(container).getByText(s.review.blocked)).toBeTruthy();
    expect(within(container).getAllByText(s.critical.title)).toHaveLength(2);
    expect(within(container).queryByText(s.critical.notImported)).toBeNull();
  });

  it("explains a refusal before anything was sent", async () => {
    const refusal: AppError = {
      kind: "invalid_request",
      detail: { detail: "edit", problem: { problem: "no_changes" } },
      message: "nothing to import: no category chosen",
    };
    await mount("import_summary", {
      prepare_import: () => {
        throw refusal;
      },
    });
    await openImport();
    fireEvent.click(screen.getByRole("button", { name: s.import.review }));
    await screen.findByText(s.refusals.no_changes);
    expect(screen.getByText(en.flow.nothingWritten)).toBeTruthy();
    expect(named("apply_import")).toHaveLength(0);
  });

  it("says when a file can't be used", async () => {
    const invalid: AppError = {
      kind: "invalid_backup",
      detail: { detail: "unsupported_schema", found: 9, supported: 2 },
      message: "unsupported schema 9",
    };
    await mount("import_summary", {
      open_import_file: () => {
        throw invalid;
      },
    });
    fireEvent.click(screen.getByRole("button", { name: s.import.open }));
    await screen.findByText(en.errors.detail.unsupported_schema({ found: 9, supported: 2 }));
    expect(screen.queryByRole("button", { name: s.import.review })).toBeNull();
  });

  it("needs “Import anyway” for a file made with other firmware", async () => {
    const summary = recordedSummary();
    const other: ImportSummaryView = { ...summary, file: { ...summary.file, firmware: "3.10.01", same_firmware: false } };
    await mount("import_summary", { open_import_file: () => other });
    await openImport();
    expect(screen.getByText(s.import.firmwareTitle)).toBeTruthy();
    const review = screen.getByRole("button", { name: s.import.review });
    expect(review).toHaveProperty("disabled", true);
    expect(screen.getByText(s.reasons.firmwareTick)).toBeTruthy();
    fireEvent.click(screen.getByLabelText(s.import.firmwareAnyway));
    expect(review).toHaveProperty("disabled", false);
    fireEvent.click(review);
    await waitFor(() => {
      expect(named("prepare_import")).toHaveLength(1);
    });
    expect(named("prepare_import")[0]?.args["intent"]).toMatchObject({ allow_firmware_mismatch: true, confirmed_switches: [] });
  });

  it("refuses a file for another keyboard", async () => {
    const summary = recordedSummary();
    const other: ImportSummaryView = { ...summary, file: { ...summary.file, model: "Wraith W60", same_model: false } };
    await mount("import_summary", { open_import_file: () => other });
    await openImport();
    expect(screen.getByText(s.import.otherKeyboardTitle)).toBeTruthy();
    expect(screen.getByRole("button", { name: s.import.review })).toHaveProperty("disabled", true);
    expect(screen.getByText(s.reasons.otherKeyboard)).toBeTruthy();
  });
});

describe("without a keyboard", () => {
  it("disables export and the import review with the reason, but opens files", async () => {
    await mount("searching");
    expect(screen.getByRole("button", { name: s.export.button({ n: 1 }) })).toHaveProperty("disabled", true);
    await openImport();
    expect(screen.getByRole("button", { name: s.import.review })).toHaveProperty("disabled", true);
    expect(screen.getAllByText(t.reasons.notConnected).length).toBeGreaterThan(0);
    const deviceCommands = calls.filter(
      (c) =>
        c.command.startsWith("prepare_") ||
        c.command.startsWith("apply_") ||
        c.command === "export_profile" ||
        c.command === "choose_export_destination",
    );
    expect(deviceCommands).toEqual([]);
  });
});

describe("in Turkish", () => {
  it("shows the share section in Turkish", async () => {
    await mount("import_summary");
    act(() => {
      setLang("tr");
    });
    const tr = DICTIONARIES.tr.backupRestore.share;
    expect(screen.getByRole("heading", { name: tr.title })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: tr.import.open }));
    await screen.findByText("A tuşuna basılır, ardından 20 ms beklenir");
    const importPanel = screen.getByRole("region", { name: tr.import.title });
    expect(within(importPanel).getByLabelText(tr.parts.keys)).toBeTruthy();
    const exportPanel = screen.getByRole("region", { name: tr.export.title });
    expect(within(exportPanel).getByLabelText(tr.export.sources.automatic)).toBeTruthy();
    expect(within(exportPanel).getByText(tr.export.partsHint)).toBeTruthy();
    expect(screen.getByRole("button", { name: tr.export.button({ n: 1 }) })).toBeTruthy();
  });
});
