// The automatic backups on the mocked IPC, replayed from the fixtures the
// core recorded on the simulator: the list (time, profile, reason, newest
// first, "newest N per profile; kept ones and safety backups are never
// deleted"), Keep (a pin, also undone), Show in folder, Restore… into the
// restore wizard and its standard review, the disabled state when automatic
// backups are off, an empty list, and the backups of every keyboard before
// one was read. Nothing here talks to the keyboard.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../../i18n";
import { makeFormat } from "../../i18n/format";
import type { AppError, AutoBackupsView, BackupFileSummary, RestorePreview } from "../../ipc/bindings";
import { createMockBackend } from "../../preview/mockBackend";
import { FIXTURES } from "../../preview/registry";
import { appInfoStore, DEFAULT_APP_INFO } from "../../state/appInfo";
import { acknowledgeVendor, resetSession } from "../../state/session";
import { connectEvents, INITIAL_STATUS, statusStore } from "../../state/status";
import { clearToasts } from "../../state/toasts";
import BackupRestorePage from "./index";

const en = DICTIONARIES.en;
const t = en.backupRestore;
const a = t.autoBackups;
const fmt = makeFormat("en");
const OLDEST = "f-6f104f2bea9cf264";
const DATE = fmt.dateTimeSeconds("2026-10-24T07:30:00Z");

interface Call {
  command: string;
  args: Record<string, unknown>;
}

let calls: Call[] = [];

function named(command: string): Call[] {
  return calls.filter((call) => call.command === command);
}

/** Answers that replace the fixture's for some commands. */
type Overrides = Partial<Record<string, (args: Record<string, unknown>) => unknown>>;

/** Mounts the page on a recorded fixture (fallbacks on, at once). */
async function mount(fixtureId: string, overrides: Overrides = {}, hash = "#/backup"): Promise<void> {
  const fixture = FIXTURES[fixtureId];
  if (fixture === undefined) throw new Error(`no fixture ${fixtureId}`);
  appInfoStore.set(fixture.app_info);
  const backend = createMockBackend(fixture, { speed: 0, replayEvents: false, fallback: true, library: FIXTURES });
  mockIPC((command, args) => {
    const record = (args ?? {}) as Record<string, unknown>;
    calls.push({ command, args: record });
    const override = overrides[command];
    return override === undefined ? backend.handle(command, args) : override(record);
  });
  await act(async () => {
    await connectEvents();
  });
  window.location.hash = hash;
  render(<BackupRestorePage />);
}

function recordedList(): AutoBackupsView {
  const call = FIXTURES["auto_backups_list"]?.calls.find((c) => c.command === "list_auto_backups");
  if (call?.command !== "list_auto_backups" || call.ok === null) throw new Error("no recorded list");
  return call.ok;
}

/** The "Automatic backups" card. */
async function card(): Promise<HTMLElement> {
  const heading = await screen.findByRole("heading", { name: a.title });
  const section = heading.closest("section");
  if (section === null) throw new Error("no card");
  return section;
}

beforeEach(() => {
  setLang("en");
  statusStore.set(INITIAL_STATUS);
  resetSession();
  acknowledgeVendor();
  clearToasts();
  calls = [];
  window.location.hash = "";
});

afterEach(() => {
  clearMocks();
  statusStore.set(INITIAL_STATUS);
  appInfoStore.set(DEFAULT_APP_INFO);
});

describe("automatic backups", () => {
  it("lists them newest first with time, profile and reason, and says what is kept", async () => {
    await mount("auto_backups_list");
    const list = await card();
    await within(list).findByText(a.intro({ keep: 10 }));
    const rows = within(list).getAllByRole("row").slice(1);
    expect(rows).toHaveLength(3);
    const reasons = rows.map((row) => within(row).getAllByRole("cell")[1]?.textContent);
    expect(reasons).toEqual([a.reasons.changed_elsewhere, a.reasons.after_write, a.reasons.first_read]);
    for (const row of rows) {
      expect(within(row).getByRole("rowheader").textContent).toBe(DATE);
      expect(within(row).getByText(en.common.profile({ n: 1 }))).toBeTruthy();
    }
    expect(within(list).getAllByRole("button", { name: a.restoreRow({ n: 1, date: DATE }) })).toHaveLength(3);
    expect(within(list).queryByText(a.off)).toBeNull();
    expect(within(list).queryByText(a.everyKeyboard)).toBeNull();
    // Listing reads file names only.
    expect(calls.some((c) => c.command.startsWith("prepare_") || c.command.startsWith("apply_"))).toBe(false);
  });

  it("keeps one (a pressed toggle) and lets it go again", async () => {
    await mount("auto_backups_list", {
      pin_auto_backup: (args) => {
        const view = recordedList();
        return {
          ...view,
          entries: view.entries.map((e) => (e.file.token === args["file"] ? { ...e, pinned: args["pinned"] === true } : e)),
        };
      },
    });
    const list = await card();
    await within(list).findByText(a.intro({ keep: 10 }));
    const keeps = within(list).getAllByRole("button", { name: a.keepRow({ n: 1, date: DATE }) });
    const oldest = keeps[2];
    if (oldest === undefined) throw new Error("no row");
    expect(oldest.getAttribute("aria-pressed")).toBe("false");
    expect(within(list).queryByText(a.kept)).toBeNull();

    fireEvent.click(oldest);
    await within(list).findByText(a.kept);
    expect(named("pin_auto_backup").map((c) => c.args)).toEqual([{ file: OLDEST, pinned: true }]);
    await waitFor(() => {
      expect(within(list).getAllByRole("button", { name: a.keepRow({ n: 1, date: DATE }) })[2]?.getAttribute("aria-pressed")).toBe(
        "true",
      );
    });

    const pressed = within(list).getAllByRole("button", { name: a.keepRow({ n: 1, date: DATE }) })[2];
    if (pressed === undefined) throw new Error("no row");
    fireEvent.click(pressed);
    await waitFor(() => {
      expect(within(list).queryByText(a.kept)).toBeNull();
    });
    expect(named("pin_auto_backup").map((c) => c.args)).toEqual([
      { file: OLDEST, pinned: true },
      { file: OLDEST, pinned: false },
    ]);
  });

  it("shows a backup in its folder, and the folder", async () => {
    await mount("auto_backups_list");
    const list = await card();
    const shows = await within(list).findAllByRole("button", { name: a.showRow({ n: 1, date: DATE }) });
    const last = shows[2];
    if (last === undefined) throw new Error("no row");
    fireEvent.click(last);
    fireEvent.click(within(list).getByRole("button", { name: a.showFolder }));
    await waitFor(() => {
      expect(named("reveal").map((c) => c.args["target"])).toEqual([
        { target: "file", file: OLDEST },
        { target: "auto_backup_dir" },
      ]);
    });
  });

  it("opens one in the restore wizard, whose review names it as the profile on that date", async () => {
    const summaryCall = FIXTURES["restore_ok_persisted"]?.calls.find((c) => c.command === "open_backup_file");
    const previewCall = FIXTURES["restore_ok_persisted"]?.calls.find((c) => c.command === "prepare_restore");
    if (summaryCall?.command !== "open_backup_file" || summaryCall.ok === null) throw new Error("no summary");
    if (previewCall?.command !== "prepare_restore" || previewCall.ok === null) throw new Error("no preview");
    const automatic: BackupFileSummary = {
      ...summaryCall.ok,
      file: { ...summaryCall.ok.file, token: OLDEST, file_name: "openwraith-W75HEARGB-20261024-073000-auto-first-read.owbackup.json" },
      kind: "automatic",
      profiles: [1],
      active_profile: 1,
    };
    const preview: RestorePreview = {
      ...previewCall.ok,
      source: automatic,
      plan: { ...previewCall.ok.plan, profiles: previewCall.ok.plan.profiles.filter((p) => p.profile === 1) },
      switches: [],
    };
    await mount("auto_backups_list", { open_safety_backup: () => automatic, prepare_restore: () => preview });
    const list = await card();
    const restores = await within(list).findAllByRole("button", { name: a.restoreRow({ n: 1, date: DATE }) });
    const oldest = restores[2];
    if (oldest === undefined) throw new Error("no row");
    fireEvent.click(oldest);

    const compare = await screen.findByRole("button", { name: t.restore.compare });
    expect(named("open_safety_backup").map((c) => c.args["file"])).toEqual([OLDEST]);
    expect(screen.getByText(t.restore.kinds.automatic)).toBeTruthy();
    fireEvent.click(compare);
    // Profile 1 is active: no switch, straight to the standard review.
    await screen.findByRole("dialog", {
      name: t.review.autoTitle({ list: "1", count: 1, date: fmt.dateTime(automatic.created_at) }),
    });
    expect(named("prepare_restore").map((c) => c.args["intent"])).toEqual([
      { source: OLDEST, profiles: null, allow_firmware_mismatch: false, confirmed_switches: [] },
    ]);
    expect(named("apply_restore")).toHaveLength(0);
  });

  it("says when automatic backups are off, and still lists the ones there are", async () => {
    await mount("auto_backups_list", { list_auto_backups: () => ({ ...recordedList(), enabled: false }) });
    const list = await card();
    await within(list).findByText(a.off);
    expect(within(list).getAllByRole("button", { name: a.restoreRow({ n: 1, date: DATE }) })).toHaveLength(3);
    fireEvent.click(within(list).getByRole("button", { name: a.openSettings }));
    expect(window.location.hash).toBe("#/settings?tab=background");
  });

  it("says when there are none yet", async () => {
    await mount("auto_backups_list", { list_auto_backups: () => ({ ...recordedList(), entries: [] }) });
    const list = await card();
    await within(list).findByText(a.empty);
    expect(within(list).queryByRole("table")).toBeNull();
  });

  it("says when the list holds every keyboard's backups (no keyboard read yet)", async () => {
    await mount("auto_backups_list", { list_auto_backups: () => ({ ...recordedList(), connected_keyboard: false }) });
    const list = await card();
    await within(list).findByText(a.everyKeyboard);
  });

  it("reads the list again after each complete read", async () => {
    const first = recordedList();
    const top = first.entries[0];
    if (top === undefined) throw new Error("no recorded entry");
    const newer = { ...first, entries: [{ ...top, file: { ...top.file, token: "f-newer" } }, ...first.entries] };
    let answers = 0;
    await mount("auto_backups_list", {
      list_auto_backups: () => {
        answers += 1;
        return answers === 1 ? first : newer;
      },
    });
    const list = await card();
    await waitFor(() => {
      expect(within(list).getAllByRole("row").slice(1)).toHaveLength(3);
    });
    const before = named("list_auto_backups").length;
    act(() => {
      const status = statusStore.get();
      statusStore.set({ ...status, snapshot_revision: (status.snapshot_revision ?? 0) + 1 });
    });
    await waitFor(() => {
      expect(within(list).getAllByRole("row").slice(1)).toHaveLength(4);
    });
    expect(named("list_auto_backups").length).toBe(before + 1);
    act(() => {
      const status = statusStore.get();
      statusStore.set({ ...status, generation: status.generation + 1 });
    });
    await waitFor(() => {
      expect(named("list_auto_backups").length).toBe(before + 2);
    });
  });

  it("says when the keyboard reports no serial number", async () => {
    await mount("auto_backups_list", { list_auto_backups: () => ({ ...recordedList(), unidentified: true }) });
    const list = await card();
    await within(list).findByText(a.unidentified);
  });

  it("is brought into view by its route (Settings, a notification)", async () => {
    await mount("auto_backups_list", {}, "#/backup?section=auto");
    const list = await card();
    await waitFor(() => {
      expect(list.contains(document.activeElement)).toBe(true);
    });
    expect(window.location.hash).toBe("#/backup");
  });

  it("says when the list can't be read", async () => {
    const failure: AppError = { kind: "io", detail: null, message: "access denied" };
    await mount("auto_backups_list", {
      list_auto_backups: () => {
        throw failure;
      },
    });
    const list = await card();
    await within(list).findByText(a.loadFailed);
  });

  it("is in Turkish", async () => {
    await mount("auto_backups_list");
    act(() => {
      setLang("tr");
    });
    const tr = DICTIONARIES.tr.backupRestore.autoBackups;
    const heading = await screen.findByRole("heading", { name: tr.title });
    const list = heading.closest("section");
    if (list === null) throw new Error("no card");
    await within(list).findByText(tr.intro({ keep: 10 }));
    expect(within(list).getByText(tr.reasons.first_read)).toBeTruthy();
    expect(within(list).getAllByRole("button", { name: tr.keepRow({ n: 1, date: makeFormat("tr").dateTimeSeconds("2026-10-24T07:30:00Z") }) })).toHaveLength(3);
  });
});
