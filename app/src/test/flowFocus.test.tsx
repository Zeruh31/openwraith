// While only a review dialog is open, nothing says "Busy" (the backend runs
// nothing); when a flow closes, the focus goes back to the control that
// opened it (it is enabled again in the same render).

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ToastRegion } from "../components/Toasts";
import { WriteFlow } from "../flow/WriteFlow";
import { DICTIONARIES, setLang } from "../i18n";
import type { DeviceStatus } from "../ipc/bindings";
import { resetDraftStore } from "../pages/lighting/draftStore";
import LightingPage from "../pages/lighting/index";
import { createMockBackend } from "../preview/mockBackend";
import { FIXTURES } from "../preview/registry";
import { ProfileSwitcher } from "../shell/Header";
import { appInfoStore, DEFAULT_APP_INFO } from "../state/appInfo";
import { isLocked } from "../state/operation";
import { acknowledgeVendor, resetSession } from "../state/session";
import { snapshotStore } from "../state/snapshot";
import { INITIAL_STATUS, statusStore } from "../state/status";
import { clearToasts } from "../state/toasts";

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

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

beforeEach(() => {
  setLang("en");
  resetSession();
  acknowledgeVendor();
  resetDraftStore();
  clearToasts();
  statusStore.set({ ...READY });
});

afterEach(() => {
  cleanup();
  clearMocks();
  statusStore.set(INITIAL_STATUS);
  snapshotStore.set({ snapshot: null, loading: false, error: null });
  appInfoStore.set(DEFAULT_APP_INFO);
  clearToasts();
});

describe("an open review dialog", () => {
  it("doesn't make the header say Busy", async () => {
    mockIPC(() => null);
    render(
      <>
        <ProfileSwitcher />
        <WriteFlow<{ token: string }, string>
          kind="trigger_edit"
          prepare={() => Promise.resolve({ token: "p-1" })}
          token={(plan) => plan.token}
          title={() => "title"}
          review={() => <p>review body</p>}
          confirmLabel={() => "confirm"}
          writesSettings
          apply={() => Promise.resolve("r")}
          result={() => null}
          onClose={() => undefined}
        />
      </>,
    );
    await screen.findByText("review body");
    expect(statusStore.get().busy).toBeNull();
    expect(isLocked()).toBe(true);
    expect(screen.queryByText(en.shell.header.switcherBlocked.busy)).toBeNull();
    expect(screen.getByText(en.common.profile({ n: 1 }))).toBeTruthy();
    // The switcher still can't open a second flow (it is behind the dialog).
    expect(screen.getByRole("radio", { name: "2", hidden: true })).toHaveProperty("disabled", true);
  });
});

describe("focus after a flow closes", () => {
  it("returns to the profile button that opened the switch", async () => {
    mockIPC((command) =>
      command === "prepare_profile_switch" ? { token: "p-1", from: 1, to: 2, nothing_to_do: false, save_wait_ms: 0 } : null,
    );
    render(<ProfileSwitcher />);
    const two = screen.getByRole("radio", { name: "2" });
    two.focus();
    fireEvent.click(two);
    await screen.findByText(en.shell.switcher.title({ n: 2 }));
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.cancel }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    await sleep(20);
    expect(two).toHaveProperty("disabled", false);
    expect(document.activeElement).toBe(two);
    expect(isLocked()).toBe(false);
  });

  it("returns to Review and write on the Lighting page", async () => {
    const source = structuredClone(FIXTURES["light_preview_mode_change"]);
    if (source === undefined) throw new Error("no fixture");
    statusStore.set(source.initial_status);
    snapshotStore.set({ snapshot: source.initial_snapshot, loading: false, error: null });
    appInfoStore.set(source.app_info);
    const backend = createMockBackend(source, { speed: 0, replayEvents: false, fallback: false, library: FIXTURES });
    mockIPC((command, args) => backend.handle(command, args));
    render(
      <>
        <LightingPage />
        <ToastRegion />
      </>,
    );
    const staticTile = screen
      .getAllByRole("radio")
      .find((radio) => radio.querySelector(".li-tile__name")?.textContent === en.lighting.modes.static);
    if (staticTile === undefined) throw new Error("no Static tile");
    fireEvent.click(staticTile);
    const review = screen.getByRole("button", { name: en.common.actions.reviewAndWrite });
    review.focus();
    fireEvent.click(review);
    await screen.findByText(en.lighting.review.title({ n: 1 }));
    fireEvent.click(screen.getByRole("button", { name: en.common.actions.cancel }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    await sleep(20);
    expect(review).toHaveProperty("disabled", false);
    expect(document.activeElement).toBe(review);
  });
});
