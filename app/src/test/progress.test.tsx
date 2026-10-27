// Progress: the timeline model, the keyboard-save wait meter in both
// languages, the Stop control's states, and the busy indicator resuming from
// the status after a reload.

import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { EMPTY_PROGRESS, reduceProgress } from "../flow/progressModel";
import { CommitWaitMeter, ProgressPanel, StopControl } from "../flow/ProgressPanel";
import { DICTIONARIES, setLang } from "../i18n";
import type { ProgressEvent, ProgressStep } from "../ipc/bindings";
import { BusyIndicator } from "../shell/Header";
import { applyStatus, INITIAL_STATUS, statusStore } from "../state/status";

afterEach(() => {
  setLang("en");
  statusStore.set(INITIAL_STATUS);
});

function event(seq: number, step: ProgressStep): ProgressEvent {
  return { op: "o-7", seq, elapsed_ms: seq * 100, step, cancel: "available" };
}

const reading = (done: number): ProgressStep => ({ step: "reading", purpose: "safety_backup", profile: null, done, total: 113 });
const waiting: ProgressStep = { step: "waiting_for_keyboard_save", profile: 2, wait_ms: 2000 };

describe("progress model", () => {
  it("groups consecutive events into phases", () => {
    const events = [
      event(1, reading(1)),
      event(2, reading(113)),
      event(3, { step: "saving_safety_backup" }),
      event(4, waiting),
    ];
    const model = events.reduce(reduceProgress, EMPTY_PROGRESS);
    expect(model.phases.map((p) => p.key)).toEqual(["reading:safety_backup:-", "saving_safety_backup", "waiting:2"]);
    expect(model.op).toBe("o-7");
    expect(model.current?.seq).toBe(4);
    expect(model.phases[0]?.seq).toBe(1);
  });
});

describe("CommitWaitMeter", () => {
  it("explains the wait in English", () => {
    setLang("en");
    render(<CommitWaitMeter waitMs={2000} profile={2} />);
    expect(screen.getByText("Waiting for the keyboard to save profile 2…")).toBeTruthy();
    expect(screen.getByText(DICTIONARIES.en.flow.progress.waitExplain)).toBeTruthy();
    expect(screen.getByRole("progressbar").getAttribute("aria-valuemax")).toBe("2000");
  });

  it("explains the wait in Turkish", () => {
    setLang("tr");
    render(<CommitWaitMeter waitMs={2000} profile={2} />);
    expect(screen.getByText("Klavyenin profil 2 ayarlarını kaydetmesi bekleniyor…")).toBeTruthy();
    expect(screen.getByText(/2 sn/)).toBeTruthy();
  });

  it("is shown by the progress panel for a save wait", () => {
    const model = [event(1, reading(113)), event(2, waiting)].reduce(reduceProgress, EMPTY_PROGRESS);
    render(<ProgressPanel model={model} />);
    expect(screen.getByText("Waiting for the keyboard to save profile 2…")).toBeTruthy();
    expect(screen.getByText(DICTIONARIES.en.flow.progress.keepPlugged)).toBeTruthy();
  });
});

describe("StopControl", () => {
  it("follows the cancel state", () => {
    const stop = vi.fn();
    const { rerender } = render(<StopControl cancel="available" requested={false} onStop={stop} />);
    screen.getByRole("button", { name: DICTIONARIES.en.flow.cancel.available }).click();
    expect(stop).toHaveBeenCalledTimes(1);
    rerender(<StopControl cancel="available" requested onStop={stop} />);
    expect(screen.getByText(DICTIONARIES.en.flow.cancel.requested)).toBeTruthy();
    rerender(<StopControl cancel="finishing_keymap_group" requested={false} onStop={stop} />);
    expect(screen.getByText(DICTIONARIES.en.flow.cancel.finishing_keymap_group)).toBeTruthy();
    rerender(<StopControl cancel="not_cancellable" requested={false} onStop={stop} />);
    expect(screen.queryByRole("button")).toBeNull();
  });
});

describe("BusyIndicator", () => {
  it("resumes the running step from the status (after a reload)", () => {
    render(<BusyIndicator />);
    expect(screen.queryByRole("status")).toBeNull();
    act(() => {
      applyStatus({
        ...INITIAL_STATUS,
        revision: 5,
        connection: { state: "ready" },
        busy: { op: "o-7", kind: "restore", last: event(9, { step: "returning_to_profile", profile: 1 }) },
      });
    });
    expect(screen.getByText("Returning to profile 1")).toBeTruthy();
    act(() => {
      applyStatus({ ...statusStore.get(), revision: 4, busy: null });
    });
    // An older status never replaces a newer one.
    expect(screen.getByText("Returning to profile 1")).toBeTruthy();
  });
});

describe("a save wait inside a phase", () => {
  it("keeps the persistence check current while its select waits, and the next phase closes it", () => {
    const steps: ProgressStep[] = [
      { step: "checking_persistence", profile: 2 },
      { step: "waiting_for_keyboard_save", profile: 2, wait_ms: 1980 },
    ];
    const model = steps.map((step, i) => event(i + 1, step)).reduce(reduceProgress, EMPTY_PROGRESS);
    expect(model.phases.map((p) => p.key)).toEqual(["checking_persistence:2"]);
    expect(model.current?.step.step).toBe("waiting_for_keyboard_save");
    render(<ProgressPanel model={model} />);
    const items = screen.getByRole("list", { name: DICTIONARIES.en.flow.progress.steps }).querySelectorAll("li");
    expect(items).toHaveLength(1);
    expect(items[0]?.textContent).toContain(DICTIONARIES.en.flow.steps.checking_persistence({ n: 2 }));
    expect(items[0]?.textContent).toContain(DICTIONARIES.en.flow.progress.current);
    // The wait's meter shows under the check.
    expect(screen.getByText("Waiting for the keyboard to save profile 2…")).toBeTruthy();
    const next = reduceProgress(model, event(3, { step: "checking_persistence", profile: 1 }));
    expect(next.phases.map((p) => p.key)).toEqual(["checking_persistence:2", "checking_persistence:1"]);
  });

  it("does the same for a profile select and a return", () => {
    for (const host of [
      { step: "selecting_profile", profile: 3 },
      { step: "returning_to_profile", profile: 1 },
    ] as const) {
      const model = [event(1, host), event(2, waiting)].reduce(reduceProgress, EMPTY_PROGRESS);
      expect(model.phases).toHaveLength(1);
      expect(model.phases[0]?.step).toEqual(host);
    }
    // A wait on its own (a write's save wait) is a phase of its own.
    const alone = [event(1, reading(113)), event(2, waiting)].reduce(reduceProgress, EMPTY_PROGRESS);
    expect(alone.phases).toHaveLength(2);
  });
});

describe("the progress live region", () => {
  it("exists before the first phase arrives, so that phase is announced", () => {
    setLang("en");
    const { rerender } = render(<ProgressPanel model={EMPTY_PROGRESS} />);
    const live = screen.getByTestId("progress-live");
    expect(live.textContent).toBe("");
    rerender(<ProgressPanel model={reduceProgress(EMPTY_PROGRESS, event(1, reading(1)))} />);
    // The same element, now with the first phase.
    expect(screen.getByTestId("progress-live")).toBe(live);
    expect(live.textContent).toBe(DICTIONARIES.en.flow.steps.reading.safety_backup);
  });

  it("keeps the Stop control's live region while its state changes", () => {
    const stop = vi.fn();
    const { rerender, container } = render(<StopControl cancel="available" requested={false} onStop={stop} />);
    const live = container.querySelector("[aria-live]");
    expect(live?.textContent).toBe("");
    rerender(<StopControl cancel="not_now" requested={false} onStop={stop} />);
    expect(container.querySelector("[aria-live]")).toBe(live);
    expect(live?.textContent).toBe(DICTIONARIES.en.flow.cancel.not_now);
    rerender(<StopControl cancel="not_cancellable" requested={false} onStop={stop} />);
    expect(container.querySelector("[aria-live]")).toBe(live);
    expect(live?.textContent).toBe(DICTIONARIES.en.flow.cancel.not_cancellable);
  });

  it("announces phases, not counters", () => {
    setLang("en");
    const first = [event(1, reading(10))].reduce(reduceProgress, EMPTY_PROGRESS);
    const { rerender } = render(<ProgressPanel model={first} />);
    const live = screen.getByTestId("progress-live");
    const before = live.textContent;
    expect(before).toBe(DICTIONARIES.en.flow.steps.reading.safety_backup);
    rerender(<ProgressPanel model={reduceProgress(first, event(2, reading(57)))} />);
    expect(screen.getByTestId("progress-live").textContent).toBe(before);
    // No other element announces itself politely.
    expect(document.querySelectorAll("[aria-live]")).toHaveLength(1);
    rerender(<ProgressPanel model={reduceProgress(first, event(3, waiting))} />);
    expect(screen.getByTestId("progress-live").textContent).toBe(DICTIONARIES.en.flow.steps.waiting_for_keyboard_save);
  });
});
