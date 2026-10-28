// The trigger tab's presets on the mocked IPC: the preview's mock backend
// answers with the fixtures the core's tests recorded from the real worker on
// the simulator (presets_*; the write itself is `apply_trigger_edit`, answered
// from trigger_apply_ok), through the real `api`, `invoke` and `Channel`.
//
// Covered: the list (built-in presets, the user's, one refused because it is
// out of the keyboard's range), preset → draft → review planned from the
// preset → write → report, editing the draft afterwards, saving the current
// values (and a refused save), deleting, blocked states.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ToastRegion } from "../../../components/Toasts";
import { DICTIONARIES, setLang } from "../../../i18n";
import type { AppError, DeviceStatus, PresetEditIntent, PresetsView } from "../../../ipc/bindings";
import { createMockBackend } from "../../../preview/mockBackend";
import { FIXTURES } from "../../../preview/registry";
import type { Fixture } from "../../../preview/types";
import { appInfoStore, DEFAULT_APP_INFO } from "../../../state/appInfo";
import { acknowledgeVendor, resetSession } from "../../../state/session";
import { snapshotStore } from "../../../state/snapshot";
import { INITIAL_STATUS, statusStore } from "../../../state/status";
import { clearToasts } from "../../../state/toasts";
import { resetActuationStore } from "../draftStore";
import ActuationPage from "../index";

const en = DICTIONARIES.en;
const t = en.actuation.presets;
const W = 46;
const Q = 45;

interface Call {
  command: string;
  args: Record<string, unknown>;
}

let calls: Call[] = [];

function fixture(id: string): Fixture {
  const found = FIXTURES[id];
  if (found === undefined) throw new Error(`no fixture ${id}`);
  return structuredClone(found);
}

function presetsOf(id: string): PresetsView {
  const call = fixture(id).calls.find((c) => c.command === "list_presets");
  if (call?.command !== "list_presets" || call.ok === null) throw new Error(`no presets in ${id}`);
  return call.ok;
}

/** Installs the fixture's backend; `apply_trigger_edit` comes from trigger_apply_ok; `answers` override commands. */
function install(
  id: string,
  options: {
    status?: Partial<DeviceStatus>;
    answers?: Record<string, (args: Record<string, unknown>) => unknown>;
    /** The fixture that answers `apply_trigger_edit` (default trigger_apply_ok). */
    writesFrom?: string;
  } = {},
): Fixture {
  const source = fixture(id);
  const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
  const writes = createMockBackend(fixture(options.writesFrom ?? "trigger_apply_ok"), {
    speed: 0,
    replayEvents: false,
    fallback: false,
    library: FIXTURES,
  });
  mockIPC((command, args) => {
    const record = (args ?? {}) as Record<string, unknown>;
    calls.push({ command, args: record });
    const answer = options.answers?.[command];
    if (answer !== undefined) return answer(record);
    if (command === "apply_trigger_edit") return writes.handle(command, args);
    return backend.handle(command, args);
  });
  statusStore.set({ ...source.initial_status, ...options.status });
  snapshotStore.set({ snapshot: structuredClone(source.initial_snapshot), loading: false, error: null });
  appInfoStore.set(source.app_info);
  return source;
}

function renderPage() {
  return render(
    <>
      <ActuationPage />
      <ToastRegion />
    </>,
  );
}

function commands(): string[] {
  return calls.map((call) => call.command);
}

function keyElement(index: number): HTMLElement {
  const element = document.querySelector<HTMLElement>(`[data-key-index="${index}"]`);
  if (element === null) throw new Error(`no key ${index}`);
  return element;
}

function clickKey(index: number): void {
  const element = keyElement(index);
  fireEvent.pointerDown(element, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
  fireEvent.pointerUp(element, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

/** Opens the presets dialog and waits for the list. */
async function openPresets(): Promise<HTMLElement> {
  fireEvent.click(screen.getByRole("button", { name: t.open }));
  const dialog = await screen.findByRole("dialog", { name: t.title });
  await within(dialog).findByRole("radio", { name: t.builtin.typing });
  return dialog;
}

function action(dialog: HTMLElement, name: string): HTMLElement {
  const actions = dialog.querySelector<HTMLElement>(".ow-dialog__actions");
  if (actions === null) throw new Error("no dialog actions");
  return within(actions).getByRole("button", { name });
}

function presetIntent(): PresetEditIntent {
  const call = calls.find((c) => c.command === "prepare_preset_edit");
  if (call === undefined) throw new Error("prepare_preset_edit was not called");
  return call.args["intent"] as PresetEditIntent;
}

beforeEach(() => {
  setLang("en");
  resetActuationStore();
  resetSession();
  acknowledgeVendor();
  calls = [];
});

afterEach(() => {
  clearMocks();
  clearToasts();
  statusStore.set(INITIAL_STATUS);
  snapshotStore.set({ snapshot: null, loading: false, error: null });
  appInfoStore.set(DEFAULT_APP_INFO);
});

describe("the presets menu", () => {
  it("lists the built-in presets and the user's, with a refused one shown, not clamped", async () => {
    install("presets_ready");
    renderPage();
    const dialog = await openPresets();
    for (const name of Object.values(t.builtin)) expect(within(dialog).getByRole("radio", { name })).toBeTruthy();
    expect(within(dialog).getByText(/2\.00 mm · RT off · Deep actuation/)).toBeTruthy();
    expect(within(dialog).getByText(/0\.80 mm · RT 0\.16 mm/)).toBeTruthy();
    expect(within(dialog).getByRole("radio", { name: "Valorant" })).toHaveProperty("disabled", false);
    const refused = within(dialog).getByRole("radio", { name: "Too deep" });
    expect(refused).toHaveProperty("disabled", true);
    const why = en.actuation.edit.problems.out_of_range({
      field: en.actuation.edit.fields.actuation,
      value: "5.00 mm",
      min: "0.10 mm",
      max: "3.40 mm",
    });
    expect(within(dialog).getByText(`5.00 mm · RT off · ${t.refused({ why })}`)).toBeTruthy();
    // Nothing chosen yet: the review can't open.
    expect(action(dialog, en.common.actions.reviewAndWrite)).toHaveProperty("disabled", true);
    expect(within(dialog).getByText(t.reasons.choose)).toBeTruthy();
    expect(commands()).toEqual(["list_presets"]);
  });

  it("fills the draft, opens the review planned from the preset, writes and reports", async () => {
    const source = install("presets_apply_ok", { writesFrom: "presets_apply_ok" });
    renderPage();
    const dialog = await openPresets();
    fireEvent.click(within(dialog).getByRole("radio", { name: t.builtin.fast }));
    fireEvent.click(within(dialog).getByRole("radio", { name: t.targets.wasd }));
    fireEvent.click(action(dialog, en.common.actions.reviewAndWrite));
    expect(await screen.findByText(en.actuation.review.title({ n: 4, profile: 1 }))).toBeTruthy();
    expect(screen.getByText(en.actuation.review.preset({ name: t.builtin.fast }))).toBeTruthy();
    const recorded = source.calls.find((c) => c.command === "prepare_preset_edit");
    expect(presetIntent()).toEqual(recorded?.args.intent);
    expect(commands()).not.toContain("prepare_trigger_edit");
    const write = await screen.findByRole("button", { name: en.common.actions.writeToKeyboard });
    await sleep(560);
    fireEvent.click(write);
    expect(await screen.findByText(en.flow.headline.doneKeys({ n: 4 }))).toBeTruthy();
    expect(commands().filter((c) => c === "apply_trigger_edit")).toHaveLength(1);
    const flow = screen.getByRole("dialog");
    fireEvent.click(action(flow, en.common.actions.close));
    expect(screen.getByText(en.actuation.apply.idle)).toBeTruthy();
  });

  it("keeps the preset's draft on Cancel; editing it afterwards plans a normal trigger edit", async () => {
    install("presets_preview", { answers: { prepare_trigger_edit: () => FIXTURES["trigger_preview"]?.calls.find((c) => c.command === "prepare_trigger_edit")?.ok } });
    renderPage();
    const dialog = await openPresets();
    fireEvent.click(within(dialog).getByRole("radio", { name: t.builtin.fast }));
    fireEvent.click(action(dialog, en.common.actions.reviewAndWrite));
    await screen.findByText(en.actuation.review.title({ n: 4, profile: 1 }));
    fireEvent.click(action(screen.getByRole("dialog"), en.common.actions.cancel));
    await waitFor(() => {
      expect(commands()).toContain("discard_plan");
    });
    expect(screen.getByText(/4 keys will change/)).toBeTruthy();
    expect(keyElement(W).textContent).toContain("0.80");
    // Changing the actuation point makes it the user's own draft.
    fireEvent.change(screen.getByRole("slider", { name: en.actuation.actuationControl.label }), { target: { value: "45" } });
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.reviewAndWrite }));
    await waitFor(() => {
      expect(commands()).toContain("prepare_trigger_edit");
    });
    expect(commands().filter((c) => c === "prepare_preset_edit")).toHaveLength(1);
  });

  it("explains a preset the backend refuses for this keyboard (never clamped)", async () => {
    // The recorded refusal of a preset out of the keyboard's range (presets_save_refused).
    const recorded = fixture("presets_save_refused").calls.find((c) => c.command === "save_user_preset")?.err;
    if (recorded === undefined || recorded === null) throw new Error("no recorded refusal");
    const refused: AppError = recorded;
    install("presets_preview", {
      answers: {
        prepare_preset_edit: () => {
          throw refused;
        },
      },
    });
    renderPage();
    const dialog = await openPresets();
    fireEvent.click(within(dialog).getByRole("radio", { name: t.builtin.typing }));
    fireEvent.click(action(dialog, en.common.actions.reviewAndWrite));
    expect(
      await screen.findByText(
        en.actuation.edit.problems.out_of_range({
          field: en.actuation.edit.fields.actuation,
          value: "5.00 mm",
          min: "0.10 mm",
          max: "3.40 mm",
        }),
      ),
    ).toBeTruthy();
    expect(screen.getByText(en.flow.nothingWritten)).toBeTruthy();
    expect(commands()).not.toContain("apply_trigger_edit");
  });

  it("is not applied while writes are blocked", async () => {
    install("presets_ready", {
      status: {
        connection: { state: "other_program", evidence: { status_reports: 1, foreign_acks: 0, profile_reports: 0 } },
        write_gate: { allowed: false, reasons: ["other_program"] },
      },
    });
    renderPage();
    const dialog = await openPresets();
    fireEvent.click(within(dialog).getByRole("radio", { name: t.builtin.fast }));
    expect(action(dialog, en.common.actions.reviewAndWrite)).toHaveProperty("disabled", true);
    expect(within(dialog).getByText(en.errors.writeBlocked.other_program)).toBeTruthy();
  });
});

describe("saving and deleting presets", () => {
  it("saves the setting of the selected key under a name", async () => {
    const recordedSave = fixture("presets_save_ok").calls.find((c) => c.command === "save_user_preset");
    if (recordedSave?.command !== "save_user_preset" || recordedSave.ok === null) throw new Error("no recorded save");
    const saved = recordedSave.ok;
    install("presets_preview", { answers: { save_user_preset: () => saved } });
    renderPage();
    clickKey(W);
    const dialog = await openPresets();
    expect(within(dialog).getByText(t.save.what({ values: "1.00 mm · RT 0.30 mm" }))).toBeTruthy();
    const save = within(dialog).getByRole("button", { name: t.save.button });
    expect(save).toHaveProperty("disabled", true);
    expect(within(dialog).getByText(t.save.nameNeeded)).toBeTruthy();
    fireEvent.change(within(dialog).getByRole("textbox", { name: t.save.name }), { target: { value: "  Mine " } });
    fireEvent.click(save);
    expect(await screen.findByText(t.saved({ name: "Mine" }))).toBeTruthy();
    const call = calls.find((c) => c.command === "save_user_preset");
    expect(call?.args["preset"]).toEqual({
      id: null,
      name: "Mine",
      actuation: 50,
      rapid_trigger: { mode: "same", sensitivity: 15 },
    });
    expect(within(dialog).getByRole("radio", { name: "Valorant" })).toBeTruthy();
  });

  it("explains a refused save", async () => {
    install("presets_save_refused");
    renderPage();
    clickKey(Q);
    const dialog = await openPresets();
    fireEvent.change(within(dialog).getByRole("textbox", { name: t.save.name }), { target: { value: "Too deep" } });
    fireEvent.click(within(dialog).getByRole("button", { name: t.save.button }));
    expect(await within(dialog).findByText(t.save.refused)).toBeTruthy();
    expect(
      within(dialog).getByText(
        en.actuation.edit.problems.out_of_range({
          field: en.actuation.edit.fields.actuation,
          value: "5.00 mm",
          min: "0.10 mm",
          max: "3.40 mm",
        }),
      ),
    ).toBeTruthy();
  });

  it("deletes a user preset", async () => {
    install("presets_ready", { answers: { delete_user_preset: () => presetsOf("presets_preview") } });
    renderPage();
    const dialog = await openPresets();
    fireEvent.click(within(dialog).getByRole("button", { name: t.delete({ name: "Valorant" }) }));
    expect(await screen.findByText(t.deleted({ name: "Valorant" }))).toBeTruthy();
    expect(calls.find((c) => c.command === "delete_user_preset")?.args["id"]).toBe("u-1");
    expect(within(dialog).getByText(t.noUser)).toBeTruthy();
  });
});
