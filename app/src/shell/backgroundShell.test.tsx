// The shell for v0.2's background features, on the statuses and notices the
// core's tests recorded: the Recovery banner (settings that look reset, a
// changed firmware, an unresolved reset report) leading to
// `#/backup?recover=1`, the banner of an OpenWraith command using the
// keyboard (with its purpose), paused game profiles (Resume), the header's
// Auto chip, the notices as toasts (also those that came before the shell),
// and routes sent by a notification click.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../i18n";
import type { DeviceStatus, Notice } from "../ipc/bindings";
import { FIXTURES } from "../preview/registry";
import { backgroundStore, resetBackgroundStore } from "../state/background";
import { profileNamesStore, resetProfileNames } from "../state/profileNames";
import {
  cliPurposeStore,
  emitNotice,
  handleAppEvent,
  INITIAL_STATUS,
  resetNoticeState,
  statusStore,
} from "../state/status";
import { clearToasts, getToasts } from "../state/toasts";
import { resetActions } from "./actions";
import { BannerStack, recoveryAttention } from "./BannerStack";
import { AutoChip, ProfileSwitcher } from "./Header";
import { NoticeToaster } from "./NoticeToaster";
import { ROUTE_EVENT, routeFromEvent, startRouteEvents } from "./routeEvents";

const en = DICTIONARIES.en;
const b = en.shell.banners;
const n = en.shell.notices;

/** The recorded statuses of a fixture, in order (initial first). */
function statuses(id: string): DeviceStatus[] {
  const fixture = FIXTURES[id];
  if (fixture === undefined) throw new Error(`no fixture ${id}`);
  return [
    fixture.initial_status,
    ...fixture.events.flatMap((e) => (e.event.event === "status" ? [e.event.status] : [])),
  ].map((status) => structuredClone(status));
}

/** The last recorded status of a fixture that matches `pick`. */
function recordedStatus(id: string, pick: (status: DeviceStatus) => boolean): DeviceStatus {
  const found = statuses(id).filter(pick).pop();
  if (found === undefined) throw new Error(`no matching status in ${id}`);
  return found;
}

/** The recorded notices of a fixture, in order. */
function notices(id: string): Notice[] {
  const fixture = FIXTURES[id];
  if (fixture === undefined) throw new Error(`no fixture ${id}`);
  return fixture.events.flatMap((e) => (e.event.event === "notice" ? [structuredClone(e.event.notice)] : []));
}

function notice<K extends Notice["notice"]>(id: string, kind: K): Extract<Notice, { notice: K }> {
  const found = notices(id).find((item): item is Extract<Notice, { notice: K }> => item.notice === kind);
  if (found === undefined) throw new Error(`no ${kind} in ${id}`);
  return found;
}

function titles(): string[] {
  return getToasts().map((toast) => toast.title);
}

beforeEach(() => {
  setLang("en");
  clearToasts();
  resetNoticeState();
  resetActions();
  resetBackgroundStore();
  resetProfileNames();
  statusStore.set(INITIAL_STATUS);
  window.location.hash = "#/device";
});

afterEach(() => {
  cleanup();
  clearMocks();
  clearToasts();
  resetNoticeState();
  resetBackgroundStore();
  resetProfileNames();
  statusStore.set(INITIAL_STATUS);
});

describe("the Recovery banner", () => {
  it("names the profiles that look reset and leads to the Recovery card, never restoring by itself", () => {
    const status = recordedStatus("recovery_reset_suspected", (s) => s.recovery.reset_suspected.length > 0);
    expect(recoveryAttention(status)).toBe(true);
    statusStore.set(status);
    render(<BannerStack />);
    const banner = screen.getByText(b.recoveryTitle).closest(".ow-banner");
    if (!(banner instanceof HTMLElement)) throw new Error("no banner");
    expect(within(banner).getByText(b.recoveryProfiles({ list: "1", count: 1 }))).toBeTruthy();
    expect(within(banner).getByText(b.recoveryReported)).toBeTruthy();
    expect(within(banner).getByText(b.recoveryNever)).toBeTruthy();
    fireEvent.click(within(banner).getByRole("button", { name: b.recoveryReview }));
    expect(window.location.hash).toBe("#/backup?recover=1");
    // On the card itself the banner steps aside, also once the page dropped `?recover=1`.
    return waitFor(() => {
      expect(screen.queryByText(b.recoveryTitle)).toBeNull();
    }).then(async () => {
      window.location.hash = "#/backup";
      await waitFor(() => {
        expect(window.location.hash).toBe("#/backup");
      });
      expect(screen.queryByText(b.recoveryTitle)).toBeNull();
    });
  });

  it("says when the firmware changed", () => {
    const status = recordedStatus("recovery_firmware_changed", (s) => s.recovery.firmware_changed);
    statusStore.set(status);
    render(<BannerStack />);
    expect(screen.getByText(b.firmwareTitle)).toBeTruthy();
    expect(screen.getByText(b.firmwareBody)).toBeTruthy();
    expect(screen.getByText(b.recoveryProfiles({ list: "1", count: 1 }))).toBeTruthy();
  });

  it("leaves the keyboard's own reset report to its banner, whose restore button opens the card", () => {
    const status = recordedStatus("recovery_reset_suspected", (s) => s.connection.state === "reset_seen");
    expect(recoveryAttention(status)).toBe(false);
    statusStore.set(status);
    render(<BannerStack />);
    expect(screen.queryByText(b.recoveryTitle)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: b.resetRestore }));
    expect(window.location.hash).toBe("#/backup?recover=1");
    expect(recoveryAttention({ ...status, connection: { state: "ready" } })).toBe(true);
    expect(recoveryAttention(INITIAL_STATUS)).toBe(false);
  });
});

describe("an OpenWraith command using the keyboard", () => {
  it("shows a banner with the command's purpose while it lasts, and the switcher says why it waits", () => {
    const paused = recordedStatus("paused_for_cli", (s) => s.paused_for_cli);
    render(
      <>
        <BannerStack />
        <ProfileSwitcher />
      </>,
    );
    // In the recorded order: the notice (with the purpose) comes before the paused status.
    act(() => {
      handleAppEvent({ event: "notice", notice: notice("paused_for_cli", "paused_for_cli") });
      const before = statuses("paused_for_cli").find((s) => !s.paused_for_cli && s.revision < paused.revision);
      if (before !== undefined) handleAppEvent({ event: "status", status: before });
      handleAppEvent({ event: "status", status: paused });
    });
    expect(screen.getByText(b.pausedForCliTitle)).toBeTruthy();
    expect(screen.getByText("set trigger")).toBeTruthy();
    expect(screen.getByText(en.shell.header.switcherBlocked.pausedForCli)).toBeTruthy();
    const released = recordedStatus("paused_for_cli", (s) => !s.paused_for_cli && s.revision > paused.revision);
    act(() => {
      handleAppEvent({ event: "status", status: released });
    });
    expect(screen.queryByText(b.pausedForCliTitle)).toBeNull();
    expect(cliPurposeStore.get()).toBeNull();
  });
});

describe("game profiles in the shell", () => {
  it("show the Auto chip while a rule holds the profile, leading to Settings > Games", () => {
    const holding = recordedStatus("games_live", (s) => s.automation.holding !== null);
    render(<AutoChip />);
    expect(screen.queryByRole("link")).toBeNull();
    act(() => {
      statusStore.set(holding);
    });
    const chip = screen.getByRole("link", { name: new RegExp(en.shell.header.auto) });
    expect(chip.getAttribute("href")).toBe("#/settings?tab=games");
    expect(chip.textContent).toContain(
      en.shell.header.autoTitle({ label: "Counter-Strike 2", profile: en.common.profile({ n: 2 }) }),
    );
    // Its explanation is read once: no tooltip repeats it.
    expect(chip.getAttribute("title")).toBeNull();
    // The user chose another profile: no chip.
    act(() => {
      statusStore.set({ ...holding, active_profile: 3 });
    });
    expect(screen.queryByRole("link")).toBeNull();
    act(() => {
      statusStore.set(recordedStatus("games_paused", (s) => s.automation.paused !== null));
    });
    expect(screen.getByRole("link", { name: new RegExp(en.shell.header.autoPaused) })).toBeTruthy();
  });

  it("put up a banner when the app paused them, with Resume", async () => {
    const resumed: unknown[] = [];
    mockIPC((command, args) => {
      if (command !== "pause_auto_profile") throw new Error(`unexpected ${command}`);
      resumed.push(args);
      return null;
    });
    const failed = recordedStatus("games_paused_failed", (s) => s.automation.paused !== null);
    statusStore.set(failed);
    render(<BannerStack />);
    expect(screen.getByText(b.autoPausedTitle)).toBeTruthy();
    expect(screen.getByText(b.autoPausedFailed)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: b.autoResume }));
    await waitFor(() => {
      expect(resumed).toEqual([{ paused: false }]);
    });
    fireEvent.click(screen.getByRole("button", { name: b.autoOpenSettings }));
    expect(window.location.hash).toBe("#/settings?tab=games");
    // The user's own pause is only the header's chip.
    cleanup();
    statusStore.set(recordedStatus("games_paused", (s) => s.automation.paused !== null));
    window.location.hash = "#/device";
    render(<BannerStack />);
    expect(screen.queryByText(b.autoPausedTitle)).toBeNull();
  });
});

describe("notices", () => {
  it("toast the recovery notices", () => {
    render(<NoticeToaster />);
    act(() => {
      emitNotice(notice("recovery_reset_suspected", "reset_suspected"));
      emitNotice(notice("recovery_firmware_changed", "firmware_changed"));
      emitNotice({ notice: "auto_backup_failed", profile: 2, message: "disk full" });
    });
    expect(titles()).toEqual([
      n.resetSuspected({ n: 1 }),
      n.firmwareChanged({ known: "3.22.07", current: "3.23.01" }),
      n.autoBackupFailed({ n: 2 }),
    ]);
    expect(getToasts()[2]?.details).toBe("disk full");
  });

  it("toast waiting and skipped switches with the reason, and an automatic pause", () => {
    render(<NoticeToaster />);
    act(() => {
      emitNotice(notice("games_deferred_drafts", "auto_switch_deferred"));
      emitNotice(notice("games_deferred_drafts", "auto_switch_skipped"));
      emitNotice(notice("games_paused_failed", "auto_profile_paused"));
      emitNotice(notice("games_paused", "auto_profile_paused"));
      emitNotice(notice("paused_for_cli", "paused_for_cli"));
    });
    const reason = en.shell.automation.waitReasons.drafts;
    expect(titles()).toEqual([
      n.autoWaiting({ label: "Counter-Strike 2", n: 2 }),
      n.autoSkipped({ label: "Counter-Strike 2", n: 2 }),
      n.autoPaused,
    ]);
    expect(getToasts().map((toast) => toast.body)).toEqual([
      n.autoWaitingBody({ reason }),
      n.autoSkippedBody({ reason }),
      n.autoPausedFailed,
    ]);
  });

  it("mention automatic switches only when game-switch notifications are on", () => {
    render(<NoticeToaster />);
    const switched = notice("games_live", "auto_switched");
    const back = notices("games_returned_to_default").filter((item) => item.notice === "auto_switched").pop();
    if (back === undefined) throw new Error("no switch back");
    act(() => {
      emitNotice(switched);
    });
    expect(titles()).toEqual([]);
    const call = FIXTURES["settings_background"]?.calls.find((c) => c.command === "get_background_settings");
    if (call?.command !== "get_background_settings" || call.ok === null) throw new Error("no recorded settings");
    backgroundStore.set({ ...call.ok, notifications: { ...call.ok.notifications, auto_switch: true } });
    profileNamesStore.set({ current_product: "W75HEARGB", products: [{ product: "W75HEARGB", names: [null, "FPS", null, null] }] });
    act(() => {
      emitNotice(switched);
      emitNotice(back);
    });
    expect(titles()).toEqual([n.autoSwitched({ label: "Counter-Strike 2", n: 2 }), n.autoSwitchedBack({ n: 1 })]);
  });

  it("show the notices that came before the shell, once", () => {
    const invalid = notice("settings_invalid", "settings_file_invalid");
    emitNotice(invalid);
    render(<NoticeToaster />);
    expect(titles()).toEqual([n.settingsInvalid]);
    expect(getToasts()[0]?.body).toBe(n.settingsInvalidKept({ file: invalid.kept_as ?? "" }));
    cleanup();
    clearToasts();
    render(<NoticeToaster />);
    expect(titles()).toEqual([]);
  });

  it("have Turkish words for every wait reason", () => {
    const tr = DICTIONARIES.tr.shell.automation.waitReasons;
    for (const [key, text] of Object.entries(en.shell.automation.waitReasons)) {
      expect(tr[key as keyof typeof tr]).not.toBe(text);
    }
  });
});

describe("route events", () => {
  it("open the app's own routes and ignore anything else", () => {
    expect(routeFromEvent("#/backup?recover=1")).toEqual({ page: "backup", params: { recover: "1" } });
    expect(routeFromEvent("/settings?tab=games")).toEqual({ page: "settings", params: { tab: "games" } });
    expect(routeFromEvent("#/nowhere")).toEqual({ page: "device", params: {} });
    expect(routeFromEvent("https://example.com/#/backup")).toBeNull();
    expect(routeFromEvent(42)).toBeNull();

    const stop = startRouteEvents();
    window.dispatchEvent(new CustomEvent(ROUTE_EVENT, { detail: "#/settings?tab=games" }));
    expect(window.location.hash).toBe("#/settings?tab=games");
    window.dispatchEvent(new CustomEvent(ROUTE_EVENT, { detail: "javascript:alert(1)" }));
    expect(window.location.hash).toBe("#/settings?tab=games");
    stop();
    window.dispatchEvent(new CustomEvent(ROUTE_EVENT, { detail: "#/backup?recover=1" }));
    expect(window.location.hash).toBe("#/settings?tab=games");
  });
});
