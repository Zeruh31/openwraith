// The confirmation flow's safety rules: Write is inert for 500 ms, Enter
// never confirms, one apply per confirmation, a plan-invalidated notice
// disables Write, the vendor tick is required on the first write, and every
// write-block reason disables Write with its text.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { WriteFlow } from "../flow/WriteFlow";
import { DICTIONARIES, setLang } from "../i18n";
import type { DeviceStatus, WriteBlockReason } from "../ipc/bindings";
import { acknowledgeVendor, resetSession, startSessionWatch } from "../state/session";
import { emitNotice, INITIAL_STATUS, statusStore } from "../state/status";

const en = DICTIONARIES.en;

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

interface Plan {
  token: string | null;
}

let calls: string[] = [];

beforeEach(() => {
  setLang("en");
  statusStore.set({ ...READY });
  resetSession();
  acknowledgeVendor();
  calls = [];
  mockIPC((command) => {
    calls.push(command);
    return null;
  });
});

afterEach(() => {
  clearMocks();
  statusStore.set(INITIAL_STATUS);
});

function setup(options: { token?: string | null; armDelayMs?: number } = {}) {
  const prepare = vi.fn(() => Promise.resolve<Plan>({ token: options.token === undefined ? "p-1" : options.token }));
  const apply = vi.fn(() => Promise.resolve("report"));
  const onClose = vi.fn();
  render(
    <WriteFlow<Plan, string>
      kind="trigger_edit"
      prepare={prepare}
      token={(plan) => plan.token}
      title={() => "Change actuation?"}
      review={() => <p>review body</p>}
      confirmLabel={() => "Write it"}
      writesSettings
      apply={apply}
      result={(report) => <p>{report}</p>}
      onClose={onClose}
      armDelayMs={options.armDelayMs ?? 60}
    />,
  );
  return { prepare, apply, onClose };
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

describe("WriteFlow", () => {
  it("prepares once and shows the review with Cancel focused", async () => {
    const { prepare } = setup();
    await screen.findByText("review body");
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(document.activeElement?.textContent).toBe(en.common.actions.cancel);
  });

  it("keeps Write inert until it is armed, then applies exactly once", async () => {
    const { apply } = setup({ armDelayMs: 120 });
    const write = await screen.findByRole("button", { name: "Write it" });
    fireEvent.click(write);
    expect(apply).not.toHaveBeenCalled();
    await sleep(160);
    fireEvent.click(write);
    fireEvent.click(write);
    expect(apply).toHaveBeenCalledTimes(1);
    await screen.findByText("report");
  });

  it("never confirms with Enter", async () => {
    const { apply } = setup();
    const write = await screen.findByRole("button", { name: "Write it" });
    await sleep(80);
    const notPrevented = fireEvent.keyDown(write, { key: "Enter" });
    expect(notPrevented).toBe(false);
    expect(apply).not.toHaveBeenCalled();
  });

  it("disables Write after a plan-invalidated notice and offers Review again", async () => {
    const { prepare, apply } = setup();
    const write = await screen.findByRole("button", { name: "Write it" });
    await sleep(80);
    act(() => {
      emitNotice({ notice: "plan_invalidated", reason: "other_program" });
    });
    expect(write).toHaveProperty("disabled", true);
    expect(screen.getByText(en.errors.planInvalidation.other_program)).toBeTruthy();
    fireEvent.click(write);
    expect(apply).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.reviewAgain }));
    await waitFor(() => {
      expect(prepare).toHaveBeenCalledTimes(2);
    });
  });

  it("requires the vendor tick on the first write", async () => {
    resetSession();
    const { apply } = setup();
    const write = await screen.findByRole("button", { name: "Write it" });
    await sleep(80);
    expect(write).toHaveProperty("disabled", true);
    expect(screen.getByText(en.flow.review.vendorTickNeeded)).toBeTruthy();
    fireEvent.click(screen.getByLabelText(en.flow.review.vendorTick));
    expect(write).toHaveProperty("disabled", false);
    fireEvent.click(write);
    expect(apply).toHaveBeenCalledTimes(1);
  });

  it("disables Write for every write-block reason and names it", async () => {
    const reasons: WriteBlockReason[] = [
      "not_connected",
      "observing",
      "busy",
      "other_program",
      "reset_seen",
      "snapshot_stale",
      "safety_dir_unavailable",
      "fault",
    ];
    const { apply } = setup();
    const write = await screen.findByRole("button", { name: "Write it" });
    await sleep(80);
    for (const reason of reasons) {
      act(() => {
        statusStore.set({ ...READY, revision: READY.revision + 1, write_gate: { allowed: false, reasons: [reason] } });
      });
      expect(write).toHaveProperty("disabled", true);
      expect(screen.getByText(en.errors.writeBlocked[reason])).toBeTruthy();
      fireEvent.click(write);
    }
    expect(apply).not.toHaveBeenCalled();
  });

  it("discards the plan on Cancel", async () => {
    const { onClose } = setup();
    await screen.findByText("review body");
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.cancel }));
    expect(onClose).toHaveBeenCalledWith(null);
    await waitFor(() => {
      expect(calls).toContain("discard_plan");
    });
  });

  it("ends with one Close button when there is nothing to do", async () => {
    const { onClose } = setup({ token: null });
    await screen.findByText(en.flow.headline.nothingToDo);
    // One Close action; the title bar's × has its own name.
    const closes = screen.getAllByRole("button", { name: en.common.actions.close });
    expect(closes).toHaveLength(1);
    expect(screen.getByRole("button", { name: en.common.actions.closeDialog })).toBeTruthy();
    const actionClose = closes[0];
    if (actionClose === undefined) throw new Error("no Close action");
    fireEvent.click(actionClose);
    expect(onClose).toHaveBeenCalledWith(null);
  });

  it("shows the safety-backup line and the save-wait note for setting writes", async () => {
    setup();
    await screen.findByText("review body");
    expect(screen.getByText(en.flow.review.saveNote)).toBeTruthy();
    expect(screen.getByText(en.flow.review.vendorNote)).toBeTruthy();
  });

  it("asks for the vendor tick again after another program appeared, also in an open flow", async () => {
    startSessionWatch();
    resetSession();
    const { prepare, apply } = setup();
    let write = await screen.findByRole("button", { name: "Write it" });
    await sleep(80);
    fireEvent.click(screen.getByLabelText(en.flow.review.vendorTick));
    expect(write).toHaveProperty("disabled", false);
    // The vendor site starts: another-program episode, plan invalidated.
    act(() => {
      statusStore.set({
        ...READY,
        revision: READY.revision + 1,
        connection: { state: "other_program", evidence: { status_reports: 1, foreign_acks: 0, profile_reports: 0 } },
        write_gate: { allowed: false, reasons: ["other_program", "snapshot_stale"] },
      });
      emitNotice({ notice: "plan_invalidated", reason: "other_program" });
    });
    // The user closes it; the keyboard is read again.
    act(() => {
      statusStore.set({ ...READY, revision: READY.revision + 2 });
    });
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.reviewAgain }));
    await waitFor(() => {
      expect(prepare).toHaveBeenCalledTimes(2);
    });
    write = await screen.findByRole("button", { name: "Write it" });
    await sleep(80);
    const tick = screen.getByLabelText(en.flow.review.vendorTick);
    expect(tick).toHaveProperty("checked", false);
    expect(write).toHaveProperty("disabled", true);
    fireEvent.click(write);
    expect(apply).not.toHaveBeenCalled();
    fireEvent.click(tick);
    expect(write).toHaveProperty("disabled", false);
  });

  it("keeps the notes and the vendor tick out of the scrolling plan", async () => {
    resetSession();
    setup();
    await screen.findByText("review body");
    const tick = screen.getByLabelText(en.flow.review.vendorTick);
    expect(tick.closest(".ow-dialog__footer")).not.toBeNull();
    expect(tick.closest(".ow-dialog__body")).toBeNull();
    expect(screen.getByText("review body").closest(".ow-dialog__body")).not.toBeNull();
  });
});
