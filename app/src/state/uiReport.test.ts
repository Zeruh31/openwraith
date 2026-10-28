// The report of drafts and the vendor-site acknowledgement to the backend
// (`report_ui_state`, the automation's "drafts" gate and the tray's click
// rule): sent at start, on every change of either value (not for changes
// that keep them), again after every `subscribe` (which resets the
// backend's copy), and again after a failure.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import type { AppError, UiStateReport } from "../ipc/bindings";
import { FIXTURES } from "../preview/registry";
import { hasDrafts, registerDraft } from "./drafts";
import { acknowledgeVendor, resetSession, startSessionWatch } from "./session";
import { connectEvents, INITIAL_STATUS, statusStore } from "./status";
import { currentUiState, resetUiReport, startUiReport } from "./uiReport";

let reports: UiStateReport[] = [];
let failNext = 0;

function settle(ms = 10): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

beforeEach(() => {
  reports = [];
  failNext = 0;
  resetSession();
  const ready = FIXTURES["games_deferred_drafts"]?.initial_status;
  if (ready === undefined) throw new Error("no recorded status");
  mockIPC((command, args) => {
    if (command === "report_ui_state") {
      if (failNext > 0) {
        failNext -= 1;
        const refused: AppError = { kind: "internal", detail: null, message: "not now" };
        throw refused;
      }
      reports.push((args as { state: UiStateReport }).state);
      return null;
    }
    if (command === "subscribe") return ready;
    throw new Error(`unexpected ${command}`);
  });
});

afterEach(() => {
  resetUiReport();
  expect(hasDrafts()).toBe(false);
  clearMocks();
  resetSession();
  statusStore.set(INITIAL_STATUS);
});

describe("the UI state report", () => {
  it("sends the state at start and on every change of drafts or acknowledgement", async () => {
    startUiReport();
    await settle();
    expect(reports).toEqual([{ drafts: false, vendor_acknowledged: false }]);

    const remove = registerDraft("keys", () => undefined, { tab: "macros", count: 1 });
    await settle();
    // Another tab's draft, or a new count, changes nothing for the backend.
    const removeOther = registerDraft("lighting", () => undefined, { tab: "colors", count: 3 });
    const replaced = registerDraft("keys", () => undefined, { tab: "macros", count: 2 });
    await settle();
    expect(reports).toEqual([
      { drafts: false, vendor_acknowledged: false },
      { drafts: true, vendor_acknowledged: false },
    ]);

    acknowledgeVendor();
    remove();
    removeOther();
    await settle();
    // The newer registration of Keys > Macros is still there.
    expect(currentUiState()).toEqual({ drafts: true, vendor_acknowledged: true });
    expect(reports.at(-1)).toEqual({ drafts: true, vendor_acknowledged: true });
    replaced();
    await settle();
    expect(reports.at(-1)).toEqual({ drafts: false, vendor_acknowledged: true });
    expect(reports).toHaveLength(4);
  });

  it("sends again after every subscribe, which resets the backend's copy", async () => {
    startUiReport();
    await settle();
    acknowledgeVendor();
    await settle();
    expect(reports).toHaveLength(2);
    await connectEvents();
    await settle();
    expect(reports).toEqual([
      { drafts: false, vendor_acknowledged: false },
      { drafts: false, vendor_acknowledged: true },
      { drafts: false, vendor_acknowledged: true },
    ]);
  });

  it("forgets the acknowledgement when another program appears, and retries a failed report", async () => {
    startSessionWatch();
    acknowledgeVendor();
    failNext = 1;
    startUiReport();
    await settle();
    expect(reports).toEqual([]);
    // Retried after 2 s.
    await settle(2100);
    expect(reports).toEqual([{ drafts: false, vendor_acknowledged: true }]);
    const other = FIXTURES["games_deferred_other_program"]?.events
      .flatMap((e) => (e.event.event === "status" ? [e.event.status] : []))
      .find((status) => status.connection.state === "other_program");
    if (other === undefined) throw new Error("no recorded other-program status");
    statusStore.set(structuredClone(other));
    await settle();
    expect(reports.at(-1)).toEqual({ drafts: false, vendor_acknowledged: false });
  });
});
