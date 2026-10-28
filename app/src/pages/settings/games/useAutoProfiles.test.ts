// The Games tab's data hook: a fetch that would answer with the state from
// before a save never replaces the save's answer.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import type { AutoProfileRulesIntent, AutoProfileStateView } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import { useAutoProfiles } from "./useAutoProfiles";

function recordedState(): AutoProfileStateView {
  const call = FIXTURES["games_settings"]?.calls.find((c) => c.command === "get_auto_profile_state");
  if (call?.command !== "get_auto_profile_state" || call.ok === null) throw new Error("no recorded state");
  const state: AutoProfileStateView = structuredClone(call.ok);
  return state;
}

afterEach(() => {
  clearMocks();
});

describe("useAutoProfiles", () => {
  it("never lets a fetch from during a save replace the save's answer", async () => {
    const before = { ...recordedState(), default_profile: 4 };
    const after = { ...recordedState(), default_profile: 1 };
    let stored = before;
    let finish: (() => void) | null = null;
    let fetches = 0;
    mockIPC((cmd) => {
      if (cmd === "get_auto_profile_state") {
        fetches += 1;
        return stored;
      }
      if (cmd === "set_auto_profile_rules") {
        return new Promise((resolve) => {
          finish = () => {
            stored = after;
            resolve(after);
          };
        });
      }
      return null;
    });
    const { result } = renderHook(() => useAutoProfiles());
    await waitFor(() => {
      expect(result.current.view?.default_profile).toBe(4);
    });
    const intent: AutoProfileRulesIntent = { enabled: after.enabled, default_profile: 1, rules: [] };
    let saved: Promise<unknown> = Promise.resolve();
    act(() => {
      saved = result.current.save(intent);
    });
    await waitFor(() => {
      expect(finish).not.toBeNull();
    });
    // The poll (or a notice) while the save runs: no fetch.
    const during = fetches;
    act(() => {
      result.current.reload();
    });
    expect(fetches).toBe(during);
    await act(async () => {
      finish?.();
      await saved;
    });
    await waitFor(() => {
      expect(fetches).toBe(during + 1);
    });
    expect(result.current.view?.default_profile).toBe(1);
  });
});
