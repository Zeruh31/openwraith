// Settings > Background on the mocked IPC, answered with the settings the
// core's tests recorded (settings_background, settings_background_task_manager):
// the defaults, start with Windows only with the notification-area icon
// (disabled with its reason), each switch saved at once with the whole
// intent, "keep N" checked before it is sent (1-100), a shell error kept on
// screen, the worker's refusal, and the lasting callout while Windows won't
// do what the switch says (Task Manager, a missing entry, an unreadable one,
// one left behind with or without the icon), read again on "Check again" and
// when the window gets the focus back.

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
import { parseKeep, windowsStartProblem } from "./BackgroundTab";
import SettingsPage from "./index";

const en = DICTIONARIES.en;
const t = en.settings.background;

let sent: BackgroundSettingsIntent[] = [];
let reads = 0;

function recordedView(fixture = "settings_background"): BackgroundSettingsView {
  const call = FIXTURES[fixture]?.calls.find((c) => c.command === "get_background_settings");
  if (call?.command !== "get_background_settings" || call.ok === null) throw new Error("no recorded settings");
  return structuredClone(call.ok);
}

function recordedDefaults(): BackgroundSettingsView {
  return recordedView();
}

function recordedRefusal(): AppError {
  const call = FIXTURES["settings_background"]?.calls.find((c) => c.command === "set_background_settings" && c.err !== null);
  if (call?.err == null) throw new Error("no recorded refusal");
  return structuredClone(call.err);
}

/**
 * The worker's rules for the answer: start with Windows only with the tray; the shell gets what
 * changed and says what Windows will do (a Task Manager "off" stays: OpenWraith never changes it).
 */
function applied(
  before: BackgroundSettingsView,
  intent: BackgroundSettingsIntent,
  taskManagerOff = false,
): BackgroundSettingsApplied {
  const start = intent.tray_enabled && intent.start_with_windows;
  const windowsStart = !start ? "off" : taskManagerOff ? "off_in_task_manager" : "on";
  const settings: BackgroundSettingsView = { ...before, ...intent, start_with_windows: start, windows_start: windowsStart };
  return {
    settings,
    shell: {
      tray: before.tray_enabled === intent.tray_enabled ? null : intent.tray_enabled,
      start_with_windows: before.start_with_windows === start ? null : start,
    },
    shell_error: null,
  };
}

/** What the backend answers now (tests change what Windows does between reads). */
const backend: { view: BackgroundSettingsView } = { view: recordedDefaults() };

function install(
  options: { shellError?: AppError; refuse?: AppError; view?: BackgroundSettingsView; taskManagerOff?: boolean } = {},
): void {
  backend.view = options.view ?? recordedDefaults();
  mockIPC((command, args) => {
    switch (command) {
      case "get_background_settings":
        reads += 1;
        return backend.view;
      case "set_background_settings": {
        const intent = (args as { settings: BackgroundSettingsIntent }).settings;
        sent.push(intent);
        if (options.refuse !== undefined) throw options.refuse;
        const answer = applied(backend.view, intent, options.taskManagerOff ?? false);
        backend.view = answer.settings;
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
  reads = 0;
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

  it("says lastingly when Task Manager keeps OpenWraith off, and checks again", async () => {
    const recorded = recordedView("settings_background_task_manager");
    expect(recorded).toMatchObject({ tray_enabled: true, start_with_windows: true, windows_start: "off_in_task_manager" });
    install({ view: recorded, taskManagerOff: true });
    await open();
    const tm = t.windowsStart;
    expect(screen.getByText(tm.offInTaskManagerTitle)).toBeTruthy();
    expect(screen.getByText(tm.offInTaskManagerBody)).toBeTruthy();
    expect(screen.getByRole("status").textContent).toContain(tm.offInTaskManagerTitle);
    // Turning the switch off and on doesn't change Task Manager's choice: the callout comes back.
    fireEvent.click(toggle(t.startWithWindows));
    await waitFor(() => {
      expect(screen.queryByText(tm.offInTaskManagerTitle)).toBeNull();
    });
    fireEvent.click(toggle(t.startWithWindows));
    expect(await screen.findByText(tm.offInTaskManagerTitle)).toBeTruthy();
    expect(sent.map((intent) => intent.start_with_windows)).toEqual([false, true]);
    // Turned on in Task Manager meanwhile: "Check again" reads the state anew.
    backend.view = { ...backend.view, windows_start: "on" };
    const before = reads;
    fireEvent.click(screen.getByRole("button", { name: tm.recheck }));
    await waitFor(() => {
      expect(screen.queryByText(tm.offInTaskManagerTitle)).toBeNull();
    });
    expect(reads).toBe(before + 1);
    expect(sent).toHaveLength(2);
  });

  it("reads the state again when the window gets the focus back", async () => {
    install({ view: { ...recordedView("settings_background_task_manager"), windows_start: "entry_missing" } });
    await open();
    expect(screen.getByText(t.windowsStart.entryMissingTitle)).toBeTruthy();
    expect(screen.getByText(t.windowsStart.entryMissingBody)).toBeTruthy();
    backend.view = { ...backend.view, windows_start: "on" };
    const before = reads;
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    await waitFor(() => {
      expect(screen.queryByText(t.windowsStart.entryMissingTitle)).toBeNull();
    });
    expect(reads).toBe(before + 1);
  });

  it("shows an entry left behind while the switch is off, and nothing while all is as wanted", async () => {
    install({ view: { ...recordedDefaults(), tray_enabled: true, windows_start: "still_on" } });
    await open();
    expect(toggle(t.startWithWindows).getAttribute("aria-checked")).toBe("false");
    expect(screen.getByText(t.windowsStart.stillOnTitle)).toBeTruthy();
    expect(screen.getByText(t.windowsStart.stillOnBody)).toBeTruthy();
    cleanup();
    clearMocks();
    resetBackgroundStore();
    // Left behind when the icon was turned off: the switch is disabled then, so the callout
    // doesn't point to it.
    install({ view: { ...recordedDefaults(), tray_enabled: false, start_with_windows: false, windows_start: "still_on" } });
    await open();
    expect(toggle(t.startWithWindows).disabled).toBe(true);
    expect(screen.getByText(t.windowsStart.stillOnTitle)).toBeTruthy();
    expect(screen.getByText(t.windowsStart.stillOnNoTrayBody)).toBeTruthy();
    expect(screen.queryByText(t.windowsStart.stillOnBody)).toBeNull();
    cleanup();
    clearMocks();
    resetBackgroundStore();
    install();
    await open();
    expect(screen.queryByRole("status")).toBeNull();
    fireEvent.click(toggle(t.tray));
    await waitFor(() => {
      expect(toggle(t.startWithWindows).disabled).toBe(false);
    });
    fireEvent.click(toggle(t.startWithWindows));
    await waitFor(() => {
      expect(backgroundStore.get()?.windows_start).toBe("on");
    });
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("maps every state to its callout only when it differs from the switch", () => {
    const tm = t.windowsStart;
    const on = { ...recordedDefaults(), tray_enabled: true, start_with_windows: true };
    const off = { ...recordedDefaults(), tray_enabled: true, start_with_windows: false };
    expect(windowsStartProblem({ ...on, windows_start: "on" }, tm)).toBeNull();
    expect(windowsStartProblem({ ...on, windows_start: "off_in_task_manager" }, tm)?.title).toBe(tm.offInTaskManagerTitle);
    expect(windowsStartProblem({ ...on, windows_start: "entry_missing" }, tm)?.title).toBe(tm.entryMissingTitle);
    expect(windowsStartProblem({ ...on, windows_start: "unavailable" }, tm)?.title).toBe(tm.unavailableTitle);
    // A simulator build never has an entry: nothing to fix, so no callout.
    expect(windowsStartProblem({ ...on, windows_start: "unavailable" }, tm, true)).toBeNull();
    expect(windowsStartProblem({ ...on, windows_start: "off_in_task_manager" }, tm, true)?.title).toBe(
      tm.offInTaskManagerTitle,
    );
    expect(windowsStartProblem({ ...on, windows_start: "still_on" }, tm)).toBeNull();
    expect(windowsStartProblem({ ...off, windows_start: "off" }, tm)).toBeNull();
    expect(windowsStartProblem({ ...off, windows_start: "unavailable" }, tm)).toBeNull();
    expect(windowsStartProblem({ ...off, windows_start: "off_in_task_manager" }, tm)).toBeNull();
    expect(windowsStartProblem({ ...off, windows_start: "still_on" }, tm)?.body).toBe(tm.stillOnBody);
    // Without the icon there is no start with Windows, whatever was saved.
    expect(windowsStartProblem({ ...on, tray_enabled: false, windows_start: "unavailable" }, tm)).toBeNull();
    // An entry left behind without the icon: its own text, which doesn't ask for the switch (disabled
    // then); whatever was saved.
    for (const start of [false, true]) {
      const left = windowsStartProblem({ ...off, tray_enabled: false, start_with_windows: start, windows_start: "still_on" }, tm);
      expect(left).toEqual({ title: tm.stillOnTitle, body: tm.stillOnNoTrayBody });
    }
    expect(tm.stillOnNoTrayBody).not.toMatch(/switch/i);
    // The Turkish texts are there too.
    const tr = DICTIONARIES.tr.settings.background.windowsStart;
    expect(windowsStartProblem({ ...on, windows_start: "off_in_task_manager" }, tr)?.body).toBe(tr.offInTaskManagerBody);
    expect(windowsStartProblem({ ...off, tray_enabled: false, windows_start: "still_on" }, tr)?.body).toBe(tr.stillOnNoTrayBody);
    expect(tr.stillOnNoTrayBody).not.toMatch(/anahtar/i);
    // A failed save shows next to these callouts: it gives no advice of its own that could point
    // the other way (Task Manager's choice is never a save error).
    expect(t.shellErrorBody).not.toMatch(/Task Manager/);
    expect(DICTIONARIES.tr.settings.background.shellErrorBody).not.toMatch(/Görev Yöneticisi/);
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
