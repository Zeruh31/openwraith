// A comparison that switches profiles (W1) asks for the vendor-site tick:
// in the consent dialog, and again on Retry or "Review again" when another
// program used the keyboard after the switches were agreed to (a dialog that
// stayed open across an other-program episode and a replug).

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../../i18n";
import type { BackupFileSummary, DeviceStatus, RestoreIntent } from "../../ipc/bindings";
import { resetSession, startSessionWatch, vendorTickRequired } from "../../state/session";
import { INITIAL_STATUS, statusStore } from "../../state/status";
import { RestoreCard } from "./RestoreCard";
import type { BackupSource } from "./useBackupSource";

const en = DICTIONARIES.en;
const t = en.backupRestore;

const READY: DeviceStatus = {
  ...INITIAL_STATUS,
  revision: 10,
  generation: 1,
  connection: { state: "ready" },
  active_profile: 1,
  firmware_tested: true,
  write_gate: { allowed: true, reasons: [] },
  snapshot_revision: 1,
};

const SUMMARY: BackupFileSummary = {
  active_profile: 1,
  created_at: "2026-10-24T07:30:00Z",
  file: { display_path: "C:\\x\\all.owbackup.json", file_name: "all.owbackup.json", token: "f-1" },
  firmware: "3.22.07",
  kind: "user",
  made_by_simulator: true,
  model: "Wraith W75",
  product: "W75HEARGB",
  profiles: [1, 2, 3, 4],
  safety_reason: null,
  same_firmware: true,
  same_model: true,
  schema_version: 2,
  tool: { name: "openwraith-app-sim", version: "0.1.0" },
  undecoded: 0,
};

const SOURCE: BackupSource = {
  summary: SUMMARY,
  loadId: 1,
  loading: false,
  error: null,
  openFile: () => undefined,
  openSafety: () => undefined,
  recheck: () => undefined,
};

let intents: RestoreIntent[] = [];

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

beforeEach(() => {
  setLang("en");
  statusStore.set({ ...READY });
  startSessionWatch();
  resetSession();
  intents = [];
  mockIPC((command, args) => {
    if (command === "prepare_restore") {
      intents.push((args as { intent: RestoreIntent }).intent);
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- the IPC rejects with an AppError
      throw { kind: "other_program_active", detail: null, message: "another program used the keyboard" };
    }
    return null;
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
  statusStore.set(INITIAL_STATUS);
});

describe("restore: Retry after another program used the keyboard", () => {
  it("asks for the vendor tick again before the comparison switches profiles", async () => {
    render(<RestoreCard source={SOURCE} demo={undefined} autoCompare={false} onRestored={() => undefined} />);
    fireEvent.click(screen.getByRole("button", { name: t.restore.compare }));
    const consent = await screen.findByRole("dialog", { name: t.restore.consentTitle });
    fireEvent.click(within(consent).getByLabelText(en.flow.review.vendorTick));
    await sleep(560);
    fireEvent.click(within(consent).getByRole("button", { name: t.restore.consentConfirm }));
    await waitFor(() => {
      expect(intents).toHaveLength(1);
    });
    expect(intents[0]?.confirmed_switches).toEqual([2, 3, 4]);
    const retry = await screen.findByRole("button", { name: en.common.actions.retry });

    // Another program shows up (a new episode: the tick is needed again)...
    act(() => {
      statusStore.set({
        ...READY,
        revision: 11,
        connection: { state: "other_program", evidence: { status_reports: 1, foreign_acks: 0, profile_reports: 0 } },
        write_gate: { allowed: false, reasons: ["other_program"] },
      });
    });
    // ...and after an unplug and replug the keyboard is ready again.
    act(() => {
      statusStore.set({ ...READY, revision: 12, generation: 2 });
    });
    expect(vendorTickRequired()).toBe(true);

    // Retry in the dialog that stayed open: the consent dialog comes back,
    // unticked, and nothing is prepared until it is ticked and confirmed.
    fireEvent.click(retry);
    const again = await screen.findByRole("dialog", { name: t.restore.consentTitle });
    expect(intents).toHaveLength(1);
    const box = within(again).getByLabelText(en.flow.review.vendorTick);
    expect(box).toHaveProperty("checked", false);
    await sleep(560);
    fireEvent.click(within(again).getByRole("button", { name: t.restore.consentConfirm }));
    expect(intents).toHaveLength(1);
    fireEvent.click(box);
    fireEvent.click(within(again).getByRole("button", { name: t.restore.consentConfirm }));
    await waitFor(() => {
      expect(intents).toHaveLength(2);
    });
    expect(intents[1]?.confirmed_switches).toEqual([2, 3, 4]);
  });
});
