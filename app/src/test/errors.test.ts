// The API client: every rejection is an `AppError`; anything else becomes
// one of kind `internal`.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";

import type { AppError } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { isAppError, toAppError } from "../ipc/errors";

afterEach(() => {
  clearMocks();
});

const STALE: AppError = {
  kind: "stale_plan",
  detail: { detail: "plan_invalidated", reason: "expired" },
  message: "the plan expired",
};

describe("toAppError", () => {
  it("keeps AppErrors", () => {
    expect(isAppError(STALE)).toBe(true);
    expect(toAppError(STALE)).toBe(STALE);
  });

  it("wraps everything else as internal", () => {
    expect(toAppError(new Error("boom"))).toEqual({ kind: "internal", detail: null, message: "boom" });
    expect(toAppError("text")).toEqual({ kind: "internal", detail: null, message: "text" });
    expect(toAppError(undefined)).toEqual({ kind: "internal", detail: null, message: "unknown error" });
    expect(toAppError({ kind: 3 })).toEqual({ kind: "internal", detail: null, message: '{"kind":3}' });
    expect(isAppError({ kind: "io", message: "x", detail: "no" })).toBe(false);
  });
});

describe("api", () => {
  it("passes arguments under their single-word names and returns the result", async () => {
    const seen: { command: string; args: unknown }[] = [];
    mockIPC((command, args) => {
      seen.push({ command, args });
      return command === "prepare_profile_switch"
        ? { token: "p-1", from: 1, to: 3, nothing_to_do: false, save_wait_ms: 0 }
        : null;
    });
    const preview = await api.prepareProfileSwitch(3);
    expect(preview.to).toBe(3);
    await api.discardPlan("p-1");
    expect(seen).toEqual([
      { command: "prepare_profile_switch", args: { profile: 3 } },
      { command: "discard_plan", args: { token: "p-1" } },
    ]);
  });

  it("rejects with the backend's AppError", async () => {
    mockIPC(() => {
      throw STALE;
    });
    await expect(api.applyTriggerEdit("p-1", () => undefined)).rejects.toEqual(STALE);
  });

  it("maps other rejections to internal", async () => {
    mockIPC(() => {
      throw new Error("IPC broke");
    });
    await expect(api.getStatus()).rejects.toEqual({ kind: "internal", detail: null, message: "IPC broke" });
  });
});
