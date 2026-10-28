// A report is never lost when its flow goes away mid-operation: the shell
// fetches it (`get_last_report`) once the operation ended and shows it with
// its safety backup and Undo; a report a flow showed is not shown again.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../i18n";
import type { EditReportView, LastReport, OpInfo } from "../ipc/bindings";
import { FIXTURES } from "../preview/registry";
import { LastReportDialog } from "../shell/LastReportDialog";
import { beginApply, markReportSeen, resetReports } from "../state/reports";
import { INITIAL_STATUS, statusStore } from "../state/status";

const en = DICTIONARIES.en;

function editReport(): EditReportView {
  const call = FIXTURES["trigger_apply_ok"]?.calls.find((c) => c.command === "apply_trigger_edit");
  if (call?.command !== "apply_trigger_edit" || call.ok === null) throw new Error("no trigger report");
  return call.ok;
}

const BUSY: OpInfo = { op: "o-9", kind: "trigger_edit", last: null };

async function flush(): Promise<void> {
  for (let i = 0; i < 5; i += 1) {
    await act(async () => {
      await Promise.resolve();
    });
  }
}

let calls: string[] = [];
let last: LastReport | null = null;

beforeEach(() => {
  setLang("en");
  resetReports();
  calls = [];
  last = null;
  statusStore.set(INITIAL_STATUS);
  mockIPC((command) => {
    calls.push(command);
    return command === "get_last_report" ? last : null;
  });
});

afterEach(() => {
  clearMocks();
  statusStore.set(INITIAL_STATUS);
  resetReports();
});

describe("LastReportDialog", () => {
  it("shows the report of an operation whose flow unmounted while it ran", async () => {
    render(<LastReportDialog />);
    await flush();
    // A flow starts a write, then goes away (page change) before the result.
    const end = beginApply();
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: BUSY });
    });
    end();
    last = { kind: "edit", report: editReport() };
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: null });
    });
    await flush();
    expect(calls.filter((c) => c === "get_last_report")).toHaveLength(2);
    expect(screen.getByText(en.shell.lastReport.title)).toBeTruthy();
    expect(screen.getByRole("button", { name: en.common.actions.undo })).toBeTruthy();
  });

  it("stays quiet while a mounted flow watches the operation, and for reports already shown", async () => {
    render(<LastReportDialog />);
    await flush();
    const end = beginApply();
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: BUSY });
    });
    last = { kind: "edit", report: editReport() };
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: null });
    });
    await flush();
    expect(calls.filter((c) => c === "get_last_report")).toHaveLength(1);
    // The flow showed it; the status may arrive after the reply.
    markReportSeen(editReport());
    end();
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: BUSY });
    });
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: null });
    });
    await flush();
    expect(screen.queryByText(en.shell.lastReport.title)).toBeNull();
  });

  it("says how to run a LOST restore again instead of offering a button it can't honour", async () => {
    const call = FIXTURES["restore_lost"]?.calls.find((c) => c.command === "apply_restore");
    if (call?.command !== "apply_restore" || call.ok === null) throw new Error("no restore report");
    expect(call.ok.headline).toBe("lost");
    render(<LastReportDialog />);
    await flush();
    const end = beginApply();
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: { ...BUSY, kind: "restore" } });
    });
    end();
    last = { kind: "restore", report: call.ok };
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: null });
    });
    await flush();
    expect(await screen.findByText(en.backupRestore.result.runAgainHint)).toBeTruthy();
    expect(screen.queryByRole("button", { name: en.backupRestore.result.runAgain })).toBeNull();
    // The window was neither closed nor reloaded: the intro says only that the page wasn't open.
    expect(screen.getByText(en.shell.lastReport.intro).textContent).not.toMatch(/reloaded|closed/);
  });

  it("shows a late import report in import words, and late reports of the new editors", async () => {
    const call = FIXTURES["import_apply_ok"]?.calls.find((c) => c.command === "apply_import");
    if (call?.command !== "apply_import" || call.ok === null) throw new Error("no import report");
    render(<LastReportDialog />);
    await flush();
    const end = beginApply();
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: { ...BUSY, kind: "import" } });
    });
    end();
    last = { kind: "import", report: call.ok };
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: null });
    });
    await flush();
    expect(await screen.findByText(/^Import complete/)).toBeTruthy();
  });

  it.each(["keymap_edit", "color_edit", "dead_band_edit", "socd_edit"] as const)(
    "shows the report of a %s whose flow went away",
    async (kind) => {
      render(<LastReportDialog />);
      await flush();
      const end = beginApply();
      act(() => {
        statusStore.set({ ...INITIAL_STATUS, busy: { ...BUSY, kind } });
      });
      end();
      last = { kind: "edit", report: { ...editReport(), safety_backup: { ...editReport().safety_backup, token: `f-${kind}` } } };
      act(() => {
        statusStore.set({ ...INITIAL_STATUS, busy: null });
      });
      await flush();
      expect(await screen.findByText(en.shell.lastReport.title)).toBeTruthy();
    },
  );

  it("never shows a report that existed before the window started", async () => {
    last = { kind: "edit", report: editReport() };
    render(<LastReportDialog />);
    await flush();
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: { ...BUSY, kind: "trigger_edit" } });
    });
    // The operation failed without a new report (e.g. a stale plan).
    act(() => {
      statusStore.set({ ...INITIAL_STATUS, busy: null });
    });
    await flush();
    expect(screen.queryByText(en.shell.lastReport.title)).toBeNull();
  });
});
