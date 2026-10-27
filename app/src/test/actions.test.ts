// Shell actions run once at a time: a double click on "Read again" sends one
// `refresh`, and a harmless repeat refused by admission is not an error toast.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";

import type { AppError } from "../ipc/bindings";
import { readAgain, recheckOtherProgram, resetActions } from "../shell/actions";
import { isLocked } from "../state/operation";
import { clearToasts, getToasts } from "../state/toasts";

afterEach(() => {
  clearMocks();
  clearToasts();
  resetActions();
});

/** Waits for pending promise callbacks. */
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("shell actions", () => {
  it("send one refresh for a double click and toast nothing", async () => {
    const calls: string[] = [];
    let finish: (value: unknown) => void = () => undefined;
    mockIPC((command) => {
      calls.push(command);
      return new Promise((resolve) => {
        finish = resolve;
      });
    });
    readAgain();
    readAgain();
    expect(isLocked()).toBe(true);
    await settle();
    expect(calls).toEqual(["refresh"]);
    finish(null);
    await settle();
    expect(isLocked()).toBe(false);
    expect(getToasts()).toEqual([]);
    // Once settled, it can run again.
    readAgain();
    await settle();
    expect(calls).toEqual(["refresh", "refresh"]);
  });

  it("keep harmless refusals quiet but report real errors", async () => {
    const running: AppError = { kind: "operation_running", detail: null, message: "busy" };
    const nothing: AppError = { kind: "invalid_request", detail: null, message: "no other program" };
    const broken: AppError = { kind: "io", detail: null, message: "disk" };
    let next = running;
    mockIPC(() => {
      throw next;
    });
    readAgain();
    await settle();
    next = nothing;
    recheckOtherProgram();
    await settle();
    expect(getToasts()).toEqual([]);
    next = broken;
    readAgain();
    await settle();
    expect(getToasts()).toHaveLength(1);
  });
});
