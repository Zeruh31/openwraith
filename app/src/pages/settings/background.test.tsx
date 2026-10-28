// Settings > Background on the mocked IPC, answered with the settings the
// core's tests recorded (settings_background): the defaults, start with
// Windows only with the notification-area icon (disabled with its reason),
// each switch saved at once with the whole intent, "keep N" checked before
// it is sent (1-100), a shell error kept on screen, and the worker's refusal.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../../i18n";
import type {
  AppError,
  BackgroundSettingsApplied,
  BackgroundSettingsIntent,
  BackgroundSettingsView,
} from "../../ipc/bindings";
import { FIXTURES } from "../../preview/registry";
import { backgroundStore, resetBackgroundStore } from "../../state/background";
import { INITIAL_STATUS, statusStore } from "../../state/status";
import { clearToasts, getToasts } from "../../state/toasts";
import { parseKeep } from "./BackgroundTab";
import SettingsPage from "./index";

const en = DICTIONARIES.en;
const t = en.settings.background;

let sent: BackgroundSettingsIntent[] = [];

function recordedDefaults(): BackgroundSettingsView {
  const call = FIXTURES["settings_background"]?.calls.find((c) => c.command === "get_background_settings");
  if (call?.command !== "get_background_settings" || call.ok === null) throw new Error("no recorded settings");
  return structuredClone(call.ok);
}

function recordedRefusal(): AppError {
  const call = FIXTURES["settings_background"]?.calls.find((c) => c.command === "set_background_settings" && c.err !== null);
  if (call?.err == null) throw new Error("no recorded refusal");
  return structuredClone(call.err);
}

/** The worker's rules for the answer: start with Windows only with the tray; the shell gets what changed. */
function applied(before: BackgroundSettingsView, intent: BackgroundSettingsIntent): BackgroundSettingsApplied {
  const start = intent.tray_enabled && intent.start_with_windows;
  const settings: BackgroundSettingsView = { ...before, ...intent, start_with_windows: start };
  return {
    settings,
    shell: {
      tray: before.tray_enabled === intent.tray_enabled ? null : intent.tray_enabled,
      start_with_windows: before.start_with_windows === start ? null : start,
    },
    shell_error: null,
  };
}

function install(options: { shellError?: AppError; refuse?: AppError } = {}): void {
  let view = recordedDefaults();
  mockIPC((command, args) => {
    switch (command) {
      case "get_background_settings":
        return view;
      case "set_background_settings": {
        const intent = (args as { settings: BackgroundSettingsIntent }).settings;
        sent.push(intent);
        if (options.refuse !== undefined) throw options.refuse;
        const answer = applied(view, intent);
        view = answer.settings;
        return { ...answer, shell_error: options.shellError ?? null };
      }
      default:
        throw new Error(`unexpected ${command}`);
    }
  });
}

async function open(): Promise<void> {
  window.location.hash = "#/settings?tab=background";
  render(<SettingsPage />);
  await screen.findByRole("switch", { name: t.tray });
}

function toggle(name: string): HTMLButtonElement {
  return screen.getByRole<HTMLButtonElement>("switch", { name });
}

beforeEach(() => {
  setLang("en");
  sent = [];
  clearToasts();
  resetBackgroundStore();
  statusStore.set({ ...INITIAL_STATUS, connection: { state: "ready" } });
});

afterEach(() => {
  cleanup();
  clearMocks();
  clearToasts();
  resetBackgroundStore();
  statusStore.set(INITIAL_STATUS);
});

describe("Settings > Background", () => {
  it("shows the recorded defaults; start with Windows needs the icon and says so", async () => {
    install();
    await open();
    expect(toggle(t.tray).getAttribute("aria-checked")).toBe("false");
    const start = toggle(t.startWithWindows);
    expect(start.disabled).toBe(true);
    expect(start.getAttribute("aria-describedby")).not.toBeNull();
    expect(screen.getByText(t.startNeedsTray)).toBeTruthy();
    expect(toggle(t.notifyReset).getAttribute("aria-checked")).toBe("true");
    expect(toggle(t.notifyAutoSwitch).getAttribute("aria-checked")).toBe("false");
    expect(toggle(t.autoBackup).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole<HTMLInputElement>("spinbutton", { name: t.keep }).value).toBe("10");
    expect(screen.getByText(t.keepHelp({ n: 10 }))).toBeTruthy();
    expect(screen.getByText(t.startUninstall)).toBeTruthy();
  });

  it("saves each switch at once with the whole intent", async () => {
    install();
    await open();
    fireEvent.click(toggle(t.tray));
    await waitFor(() => {
      expect(toggle(t.startWithWindows).disabled).toBe(false);
    });
    expect(sent).toEqual([
      {
        tray_enabled: true,
        start_with_windows: false,
        notifications: { reset: true, auto_switch: false },
        auto_backup: { enabled: true, keep: 10 },
      },
    ]);
    fireEvent.click(toggle(t.startWithWindows));
    await waitFor(() => {
      expect(toggle(t.startWithWindows).getAttribute("aria-checked")).toBe("true");
    });
    expect(sent[1]?.start_with_windows).toBe(true);
    // Turning the icon off also turns start with Windows off.
    fireEvent.click(toggle(t.tray));
    await waitFor(() => {
      expect(sent).toHaveLength(3);
    });
    expect(sent[2]).toMatchObject({ tray_enabled: false, start_with_windows: false });
    await waitFor(() => {
      expect(toggle(t.startWithWindows).disabled).toBe(true);
    });
    fireEvent.click(toggle(t.notifyAutoSwitch));
    await waitFor(() => {
      expect(backgroundStore.get()?.notifications.auto_switch).toBe(true);
    });
    expect(sent[3]?.notifications).toEqual({ reset: true, auto_switch: true });
  });

  it("checks the number of automatic backups before it is sent", async () => {
    expect(parseKeep("0")).toBeNull();
    expect(parseKeep("101")).toBeNull();
    expect(parseKeep("2.5")).toBeNull();
    expect(parseKeep(" 20 ")).toBe(20);
    install();
    await open();
    const keep = screen.getByRole("spinbutton", { name: t.keep });
    fireEvent.change(keep, { target: { value: "0" } });
    fireEvent.blur(keep);
    expect(screen.getByText(t.keepInvalid)).toBeTruthy();
    expect(keep.getAttribute("aria-invalid")).toBe("true");
    expect(sent).toEqual([]);
    fireEvent.change(keep, { target: { value: "20" } });
    fireEvent.click(screen.getByRole("button", { name: t.keepSave }));
    await waitFor(() => {
      expect(sent.map((intent) => intent.auto_backup)).toEqual([{ enabled: true, keep: 20 }]);
    });
    expect(await screen.findByText(t.keepHelp({ n: 20 }))).toBeTruthy();
  });

  it("keeps a shell error on screen and toasts a refusal", async () => {
    install({ shellError: { kind: "internal", detail: null, message: "the Run key could not be written" } });
    await open();
    fireEvent.click(toggle(t.tray));
    expect(await screen.findByText(t.shellErrorTitle)).toBeTruthy();
    expect(screen.getByText(t.shellErrorBody)).toBeTruthy();
    cleanup();
    clearMocks();
    resetBackgroundStore();
    install({ refuse: recordedRefusal() });
    await open();
    fireEvent.click(toggle(t.tray));
    await waitFor(() => {
      expect(getToasts().map((toast) => toast.title)).toEqual([en.settings.saveFailed]);
    });
    // The saved settings stay as they were.
    expect(toggle(t.tray).getAttribute("aria-checked")).toBe("false");
  });

  it("leads to the automatic backups", async () => {
    install();
    await open();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: t.showBackups }));
      await Promise.resolve();
    });
    expect(window.location.hash).toBe("#/backup?section=auto");
  });
});
