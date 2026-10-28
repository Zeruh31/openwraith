// Phase 5b-1 shell glue: the header's "Live" pill while the key-depth view
// runs, the banner for a key-depth test left open, the depth view named as
// the busy reason, edit headlines for table writes, and the one-time tips.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { Tip } from "../components/Tip";
import { busyText, writeBlockText } from "../flow/labels";
import { editHeadline } from "../flow/Report";
import { DICTIONARIES, setLang } from "../i18n";
import type { DeviceStatus, EditReportView } from "../ipc/bindings";
import { FIXTURES } from "../preview/registry";
import { navigate } from "../routes";
import { BannerStack } from "../shell/BannerStack";
import { depthViewLive, LivePill } from "../shell/Header";
import { resetHintsStore } from "../state/hints";
import { INITIAL_STATUS, statusStore } from "../state/status";

const en = DICTIONARIES.en;

const READY: DeviceStatus = {
  ...INITIAL_STATUS,
  connection: { state: "ready" },
  active_profile: 1,
  snapshot_revision: 1,
  write_gate: { allowed: true, reasons: [] },
};

const LIVE: DeviceStatus = {
  ...READY,
  busy: { op: "o-9", kind: "depth_view", last: null },
  write_gate: { allowed: false, reasons: ["busy"] },
  travel: { state: "open", stray_reports: 0, marker_left: false },
};

function recordedEdit(fixture: string, command: string): EditReportView {
  const call = FIXTURES[fixture]?.calls.find((c) => c.command === command);
  if (call === undefined || call.ok === null) throw new Error(`no ${command} in ${fixture}`);
  return call.ok as EditReportView;
}

beforeEach(() => {
  setLang("en");
  statusStore.set(READY);
  window.location.hash = "#/device";
});

afterEach(() => {
  cleanup();
  clearMocks();
  resetHintsStore();
  statusStore.set(INITIAL_STATUS);
});

describe("the key-depth view in the shell", () => {
  it("shows the Live pill only while the view runs, linking to the Key depth tab", () => {
    expect(depthViewLive(READY)).toBe(false);
    expect(depthViewLive(LIVE)).toBe(true);
    expect(depthViewLive({ ...READY, travel: { ...READY.travel, state: "open" } })).toBe(true);
    const { container } = render(<LivePill />);
    expect(container.textContent).toBe("");
    act(() => {
      statusStore.set(LIVE);
    });
    const link = screen.getByRole("link", { name: new RegExp(en.shell.header.live) });
    expect(link.getAttribute("href")).toBe("#/actuation?tab=depth");
  });

  it("names the live view as the busy reason", () => {
    expect(busyText(LIVE, en)).toBe(en.errors.depthViewRunning);
    expect(busyText({ busy: { op: "o-1", kind: "trigger_edit", last: null } }, en)).toBe(en.errors.writeBlocked.busy);
    expect(writeBlockText("busy", LIVE, en)).toBe(en.errors.depthViewRunning);
    expect(writeBlockText("observing", LIVE, en)).toBe(en.errors.writeBlocked.observing);
  });

  it("puts up a banner for a test left open, except on the Key depth tab and while a view runs", () => {
    const stray: DeviceStatus = { ...READY, travel: { state: "closed", stray_reports: 0, marker_left: true } };
    statusStore.set(stray);
    render(<BannerStack />);
    expect(screen.getByText(en.shell.banners.strayTravelTitle)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: en.shell.banners.strayTravelOpen }));
    expect(window.location.hash).toBe("#/actuation?tab=depth");
    return waitFor(() => {
      expect(screen.queryByText(en.shell.banners.strayTravelTitle)).toBeNull();
    }).then(() => {
      act(() => {
        navigate("device");
        statusStore.set({ ...stray, travel: { state: "maybe_open", stray_reports: 3, marker_left: false } });
      });
      return waitFor(() => {
        expect(screen.getByText(en.shell.banners.strayTravelTitle)).toBeTruthy();
      });
    });
  });
});

describe("edit headlines", () => {
  it("count keys for trigger edits and say 'done' for table writes", () => {
    const trigger = recordedEdit("trigger_apply_ok", "apply_trigger_edit");
    expect(editHeadline(trigger, en)).toBe(en.flow.headline.doneKeys({ n: trigger.items.length }));
    for (const [fixture, command] of [
      ["keys_apply_ok", "apply_keymap_edit"],
      ["colors_apply_ok", "apply_color_edit"],
      ["dead_band_apply_ok", "apply_dead_band_edit"],
      ["socd_apply_ok", "apply_socd_edit"],
    ] as const) {
      const report = recordedEdit(fixture, command);
      expect(report.headline, fixture).toBe("done");
      expect(editHeadline(report, en), fixture).toBe(en.flow.headline.done);
    }
  });
});

describe("one-time tips", () => {
  it("show until dismissed, remember it, and stay hidden when the list can't be read", async () => {
    let shown: string[] = [];
    const marked: string[] = [];
    mockIPC((command, args) => {
      if (command === "get_hints_shown") return shown;
      if (command === "mark_hint_shown") {
        const hint = (args as { hint: string }).hint;
        marked.push(hint);
        shown = [...shown, hint];
        return shown;
      }
      throw new Error(`unexpected ${command}`);
    });
    render(
      <Tip id="test.tip" title="Title">
        {"Body"}
      </Tip>,
    );
    fireEvent.click(await screen.findByRole("button", { name: en.common.tip.dismiss }));
    expect(screen.queryByText("Title")).toBeNull();
    await waitFor(() => {
      expect(marked).toEqual(["test.tip"]);
    });
    cleanup();
    resetHintsStore();
    clearMocks();
    mockIPC(() => {
      throw new Error("no settings");
    });
    render(
      <Tip id="other.tip" title="Hidden">
        {"Body"}
      </Tip>,
    );
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.queryByText("Hidden")).toBeNull();
  });
});

describe("Settings", () => {
  it("say where trigger presets are managed and show dismissed tips again", async () => {
    const presets = FIXTURES["presets_ready"]?.calls.find((c) => c.command === "list_presets")?.ok;
    if (presets === undefined || presets === null) throw new Error("no recorded presets");
    let shown = ["keys.intro"];
    mockIPC((command) => {
      switch (command) {
        case "list_presets":
          return presets;
        case "get_hints_shown":
          return shown;
        case "reset_hints_shown":
          shown = [];
          return shown;
        default:
          throw new Error(`unexpected ${command}`);
      }
    });
    const { default: SettingsPage } = await import("../pages/settings");
    render(<SettingsPage />);
    const t = en.settings;
    expect(await screen.findByText(t.presets.count({ n: presets.user.length }))).toBeTruthy();
    expect(await screen.findByText(t.tips.count({ n: 1 }))).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: t.tips.reset }));
    expect(await screen.findByText(t.tips.count({ n: 0 }))).toBeTruthy();
    expect(screen.getByRole<HTMLButtonElement>("button", { name: t.tips.reset }).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: t.presets.open }));
    expect(window.location.hash).toBe("#/actuation?tab=trigger");
  });
});
