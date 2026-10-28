// Settings > Games on the mocked IPC, with the views, programs and refusals
// the core's tests recorded from the real worker (games_*): the consent
// explanation (once; its tick stands for the vendor-site confirmation of
// automatic switches), adding a rule from a running program, Browse… (the
// Rust dialog) and detection (tokens only), a typed file name, the table's
// in-place edits (programs kept by id), the live line, recent activity,
// pause, the tray note and refusals.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DICTIONARIES, setLang } from "../../i18n";
import type {
  AppError,
  AutoProfileRulesIntent,
  AutoProfileRuleView,
  AutoProfileStateView,
  ProfileNamesView,
  ProgramView,
} from "../../ipc/bindings";
import type { CommandName, Results } from "../../ipc/commands";
import { FIXTURES } from "../../preview/registry";
import { resetHintsStore } from "../../state/hints";
import { profileNamesStore, resetProfileNames } from "../../state/profileNames";
import { INITIAL_STATUS, statusStore } from "../../state/status";
import { clearToasts, getToasts } from "../../state/toasts";
import { CONSENT_HINT } from "./games/GamesTab";
import SettingsPage from "./index";

const en = DICTIONARIES.en;
const t = en.settings.games;

interface Call {
  command: string;
  args: Record<string, unknown>;
}

let calls: Call[] = [];

/** The `n`-th recorded answer of `command` in fixture `id`. */
function recorded<K extends CommandName>(id: string, command: K, n = 0): Results[K] {
  const call = FIXTURES[id]?.calls.filter((c) => c.command === command)[n];
  if (call === undefined || call.ok === null) throw new Error(`no ${command} #${String(n)} in ${id}`);
  return structuredClone(call.ok) as Results[K];
}

function recordedError(id: string, command: CommandName): AppError {
  const call = FIXTURES[id]?.calls.find((c) => c.command === command && c.err !== null);
  if (call?.err == null) throw new Error(`no refused ${command} in ${id}`);
  return structuredClone(call.err);
}

const NAMES: ProfileNamesView = recorded("settings_background", "set_profile_names");

/** The view after an intent, as the worker stores it (ids r-<n>, programs by token, typed name or kept). */
function applyIntent(
  view: AutoProfileStateView,
  intent: AutoProfileRulesIntent,
  known: ReadonlyMap<string, ProgramView>,
  newId: () => string,
): AutoProfileStateView {
  const rules: AutoProfileRuleView[] = intent.rules.map((rule) => {
    const old = view.rules.find((r) => r.id === rule.id);
    let exe = old?.exe ?? "";
    let folder = old?.folder ?? null;
    let generic = old?.generic_name ?? false;
    if (rule.program.source === "picked") {
      const program = known.get(rule.program.token);
      if (program === undefined) throw new Error(`unknown token ${rule.program.token}`);
      ({ exe, folder } = program);
      generic = program.generic_name;
    } else if (rule.program.source === "name") {
      exe = rule.program.exe;
      folder = null;
      generic = false;
    }
    return {
      id: rule.id ?? newId(),
      label: rule.label,
      exe,
      folder,
      profile: rule.profile,
      enabled: rule.enabled,
      generic_name: generic,
    };
  });
  const enabled = intent.enabled;
  return {
    ...view,
    enabled,
    default_profile: intent.default_profile,
    rules,
    status: { ...view.status, enabled },
    line: !enabled ? { line: "off" } : rules.length === 0 ? { line: "no_rules" } : { line: "watching" },
  };
}

interface Setup {
  view: AutoProfileStateView;
  programs?: ProgramView[];
  picked?: ProgramView | null;
  detected?: ProgramView | null;
  hints?: string[];
  /** Answers `set_auto_profile_rules` with this error. */
  refuse?: AppError | null;
  /** Answers of `pause_auto_profile`. */
  pause?: AutoProfileStateView;
}

/** A small worker for the Games commands, built from recorded data. */
function install(setup: Setup): () => AutoProfileStateView {
  let view = structuredClone(setup.view);
  let hints = [...(setup.hints ?? [])];
  const known = new Map<string, ProgramView>();
  const offer = (program: ProgramView) => {
    known.set(program.token, program);
    return program;
  };
  let next = view.rules.length + 1;
  mockIPC((command, args) => {
    const record = (args ?? {}) as Record<string, unknown>;
    calls.push({ command, args: record });
    switch (command) {
      case "get_auto_profile_state":
        return view;
      case "set_auto_profile_rules":
        if (setup.refuse != null) throw setup.refuse;
        view = applyIntent(view, record["rules"] as AutoProfileRulesIntent, known, () => `r-${String(next++)}`);
        return view;
      case "pause_auto_profile":
        view = setup.pause ?? view;
        return view;
      case "list_running_programs":
        return (setup.programs ?? []).map(offer);
      case "pick_game_program":
        return setup.picked == null ? null : offer(setup.picked);
      case "detect_next_program":
        return setup.detected == null ? null : offer(setup.detected);
      case "get_hints_shown":
        return hints;
      case "mark_hint_shown":
        hints = [...hints, String(record["hint"])];
        return hints;
      case "get_profile_names":
        return NAMES;
      default:
        throw new Error(`unexpected ${command}`);
    }
  });
  return () => view;
}

function rulesSent(n = -1): AutoProfileRulesIntent {
  const sent = calls.filter((call) => call.command === "set_auto_profile_rules");
  const call = sent[n < 0 ? sent.length + n : n];
  if (call === undefined) throw new Error("no set_auto_profile_rules");
  return call.args["rules"] as AutoProfileRulesIntent;
}

function named(command: string): Call[] {
  return calls.filter((call) => call.command === command);
}

function sleep(ms: number): Promise<void> {
  return act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));
}

function dialog(): HTMLElement {
  return screen.getByRole("dialog");
}

function dialogButton(name: string): HTMLElement {
  const actions = dialog().querySelector<HTMLElement>(".ow-dialog__actions");
  if (actions === null) throw new Error("no dialog actions");
  return within(actions).getByRole("button", { name });
}

async function openGames(): Promise<void> {
  window.location.hash = "#/settings?tab=games";
  render(<SettingsPage />);
  await screen.findByRole("switch", { name: t.toggle });
}

const OFF = recorded("games_settings", "get_auto_profile_state");
const LIVE_RULES = recorded("games_live", "set_auto_profile_rules");
const RUNNING = recorded("games_settings", "list_running_programs");

beforeEach(() => {
  setLang("en");
  calls = [];
  clearToasts();
  resetHintsStore();
  resetProfileNames();
  profileNamesStore.set(NAMES);
  const status = FIXTURES["games_live"]?.initial_status;
  if (status === undefined) throw new Error("no games_live status");
  statusStore.set(structuredClone(status));
});

afterEach(() => {
  cleanup();
  clearMocks();
  clearToasts();
  resetHintsStore();
  resetProfileNames();
  statusStore.set(INITIAL_STATUS);
});

describe("Settings > Games", () => {
  it("shows the recorded state: off, its line, no games yet, the tray note and how it works", async () => {
    install({ view: OFF });
    await openGames();
    expect(screen.getByRole("switch", { name: t.toggle }).getAttribute("aria-checked")).toBe("false");
    expect(screen.getByRole("status").textContent).toContain(t.line.off);
    expect(screen.getByText(t.rules.empty)).toBeTruthy();
    expect(screen.getByText(t.needsTray)).toBeTruthy();
    expect(screen.getByText(t.howItWorks.title)).toBeTruthy();
    expect(screen.getByText(t.howItWorks.stops)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: t.openBackground }));
    expect(window.location.hash).toBe("#/settings?tab=background");
  });

  it("asks once before turning on; the tick stands for the vendor-site confirmation", async () => {
    install({ view: OFF });
    await openGames();
    const toggle = screen.getByRole("switch", { name: t.toggle });
    fireEvent.click(toggle);
    expect(within(dialog()).getByText(t.consent.title)).toBeTruthy();
    expect(within(dialog()).getByText(t.howItWorks.writes)).toBeTruthy();
    // Without the tick, "Turn on" says why it can't.
    const turnOn = dialogButton(t.consent.confirm);
    expect((turnOn as HTMLButtonElement).disabled).toBe(true);
    expect(within(dialog()).getByText(t.consent.needsTick)).toBeTruthy();
    fireEvent.click(within(dialog()).getByRole("button", { name: en.common.actions.cancel }));
    expect(named("set_auto_profile_rules")).toEqual([]);

    fireEvent.click(toggle);
    fireEvent.click(within(dialog()).getByRole("checkbox", { name: t.consent.tick }));
    // Enter never confirms; the button is armed after 500 ms.
    fireEvent.keyDown(dialogButton(t.consent.confirm), { key: "Enter" });
    fireEvent.click(dialogButton(t.consent.confirm));
    expect(named("set_auto_profile_rules")).toEqual([]);
    await sleep(560);
    fireEvent.click(dialogButton(t.consent.confirm));
    await waitFor(() => {
      expect(rulesSent()).toEqual({ enabled: true, default_profile: null, rules: [] });
    });
    expect(named("mark_hint_shown").map((call) => call.args["hint"])).toEqual([CONSENT_HINT]);
    await waitFor(() => {
      expect(screen.getByRole("status").textContent).toContain(t.line.noRules);
    });

    // Off and on again: no second explanation.
    fireEvent.click(screen.getByRole("switch", { name: t.toggle }));
    await waitFor(() => {
      expect(rulesSent().enabled).toBe(false);
    });
    fireEvent.click(screen.getByRole("switch", { name: t.toggle }));
    await waitFor(() => {
      expect(named("set_auto_profile_rules")).toHaveLength(3);
    });
    expect(rulesSent().enabled).toBe(true);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("adds a running program by its token, with a name and a profile, and warns about shared names", async () => {
    install({ view: OFF, programs: RUNNING, hints: [CONSENT_HINT] });
    await openGames();
    fireEvent.click(screen.getByRole("button", { name: t.rules.add }));
    const add = within(dialog());
    // Nothing chosen yet: "Add game" says why.
    expect(add.getByText(t.add.needProgram)).toBeTruthy();
    fireEvent.click(add.getByRole("button", { name: t.add.runningShow }));
    const list = await add.findByRole("group", { name: t.add.running });
    const javaw = RUNNING.find((p) => p.generic_name);
    if (javaw === undefined) throw new Error("no generic program recorded");
    fireEvent.click(within(list).getByRole("radio", { name: javaw.exe }));
    expect(add.getByText(en.settings.games.rules.generic)).toBeTruthy();
    const name = add.getByRole<HTMLInputElement>("textbox", { name: t.add.name });
    expect(name.value).toBe("javaw");
    fireEvent.change(name, { target: { value: "Minecraft" } });
    fireEvent.change(add.getByRole("combobox", { name: t.add.profile }), { target: { value: "3" } });
    fireEvent.click(dialogButton(t.add.confirm));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(rulesSent().rules).toEqual([
      { id: null, label: "Minecraft", program: { source: "picked", token: javaw.token }, profile: 3, enabled: true },
    ]);
    const row = screen.getByRole("row", { name: /Minecraft/ });
    expect(within(row).getByText(javaw.exe)).toBeTruthy();
    expect(within(row).getByText(en.settings.games.rules.generic)).toBeTruthy();
    // The local profile names label the profiles.
    expect(
      within(row).getByRole<HTMLSelectElement>("combobox", { name: t.rules.profileFor({ label: "Minecraft" }) }).value,
    ).toBe("3");
    expect(within(row).getByRole("option", { name: "Profile 2 · FPS" })).toBeTruthy();
  });

  it("takes a program from the Rust file dialog or the next program brought to the front", async () => {
    const picked = recorded("games_settings", "pick_game_program");
    const detected = recorded("games_settings", "detect_next_program");
    if (picked === null || detected === null) throw new Error("no recorded programs");
    install({ view: OFF, picked, detected, hints: [CONSENT_HINT] });
    await openGames();
    fireEvent.click(screen.getByRole("button", { name: t.rules.add }));
    const add = within(dialog());
    fireEvent.click(add.getByRole("button", { name: t.add.browse }));
    // The dialog's words come from the dictionary, as the worker recorded them.
    await waitFor(() => {
      expect(named("pick_game_program").map((call) => call.args)).toEqual([
        FIXTURES["games_settings"]?.calls.find((c) => c.command === "pick_game_program")?.args,
      ]);
    });
    expect(await add.findByText(picked.exe)).toBeTruthy();
    expect(add.getByText(t.add.folder({ folder: picked.folder ?? "" }))).toBeTruthy();

    fireEvent.click(add.getByRole("button", { name: t.add.detect }));
    expect(await add.findByText(detected.exe)).toBeTruthy();
    expect(add.getByRole<HTMLInputElement>("textbox", { name: t.add.name }).value).toBe("VALORANT");
    fireEvent.click(dialogButton(t.add.confirm));
    await waitFor(() => {
      expect(rulesSent().rules[0]?.program).toEqual({ source: "picked", token: detected.token });
    });
  });

  it("refuses a typed file name with a folder and adds one without", async () => {
    install({ view: OFF, hints: [CONSENT_HINT] });
    await openGames();
    fireEvent.click(screen.getByRole("button", { name: t.rules.add }));
    const add = within(dialog());
    const typed = add.getByRole("textbox", { name: t.add.typeName });
    fireEvent.change(typed, { target: { value: "C:\\Games\\x.exe" } });
    fireEvent.click(add.getByRole("button", { name: t.add.useName }));
    // Said at the field and as the reason "Add game" is unavailable.
    expect(add.getAllByText(t.add.typeNameInvalid)).toHaveLength(2);
    expect(add.getByText(t.add.none)).toBeTruthy();
    fireEvent.change(typed, { target: { value: "game.exe" } });
    fireEvent.keyDown(typed, { key: "Enter" });
    expect(add.getByText("game.exe")).toBeTruthy();
    fireEvent.click(dialogButton(t.add.confirm));
    await waitFor(() => {
      expect(rulesSent().rules).toEqual([
        { id: null, label: "game", program: { source: "name", exe: "game.exe" }, profile: 2, enabled: true },
      ]);
    });
  });

  it("takes a typed name without “Use this name”, as the .exe of that name", async () => {
    install({ view: OFF, hints: [CONSENT_HINT] });
    await openGames();
    fireEvent.click(screen.getByRole("button", { name: t.rules.add }));
    const add = within(dialog());
    fireEvent.change(add.getByRole("textbox", { name: t.add.typeName }), { target: { value: "VALORANT" } });
    expect(add.getByRole("textbox", { name: t.add.name })).toHaveProperty("value", "VALORANT");
    fireEvent.click(dialogButton(t.add.confirm));
    await waitFor(() => {
      expect(rulesSent().rules).toEqual([
        { id: null, label: "VALORANT", program: { source: "name", exe: "VALORANT.exe" }, profile: 2, enabled: true },
      ]);
    });
  });

  it("shows the worker's refusal in the dialog and keeps it open", async () => {
    install({ view: OFF, hints: [CONSENT_HINT], refuse: recordedError("games_settings", "set_auto_profile_rules") });
    await openGames();
    fireEvent.click(screen.getByRole("button", { name: t.rules.add }));
    const add = within(dialog());
    fireEvent.change(add.getByRole("textbox", { name: t.add.typeName }), { target: { value: "bad.exe" } });
    fireEvent.click(add.getByRole("button", { name: t.add.useName }));
    fireEvent.click(dialogButton(t.add.confirm));
    expect(await add.findByText(t.add.failed)).toBeTruthy();
    expect(add.getByText(en.errors.kind.invalid_request.title)).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("edits rules in place and keeps their programs", async () => {
    install({ view: LIVE_RULES });
    await openGames();
    const cs = screen.getByRole("row", { name: /Counter-Strike 2/ });
    fireEvent.change(within(cs).getByRole("combobox", { name: t.rules.profileFor({ label: "Counter-Strike 2" }) }), {
      target: { value: "4" },
    });
    await waitFor(() => {
      expect(named("set_auto_profile_rules")).toHaveLength(1);
    });
    expect(rulesSent()).toEqual({
      enabled: true,
      default_profile: 1,
      rules: LIVE_RULES.rules.map((rule) => ({
        id: rule.id,
        label: rule.label,
        program: { source: "keep" },
        profile: rule.id === "r-1" ? 4 : rule.profile,
        enabled: rule.enabled,
      })),
    });

    await waitFor(() => {
      expect(screen.getByRole<HTMLButtonElement>("switch", { name: t.rules.ruleOn({ label: "Racing" }) }).disabled).toBe(false);
    });
    fireEvent.click(screen.getByRole("switch", { name: t.rules.ruleOn({ label: "Racing" }) }));
    await waitFor(() => {
      expect(rulesSent().rules.find((rule) => rule.id === "r-3")?.enabled).toBe(true);
    });

    await waitFor(() => {
      expect(screen.getByRole<HTMLButtonElement>("button", { name: t.rules.removeRule({ label: "Minecraft" }) }).disabled).toBe(
        false,
      );
    });
    fireEvent.click(screen.getByRole("button", { name: t.rules.removeRule({ label: "Minecraft" }) }));
    await waitFor(() => {
      expect(rulesSent().rules.map((rule) => rule.id)).toEqual(["r-1", "r-3"]);
    });
    await waitFor(() => {
      expect(getToasts().map((toast) => toast.title)).toContain(t.rules.removed({ label: "Minecraft" }));
    });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: t.rules.add }));

    const back = screen.getByRole<HTMLSelectElement>("combobox", { name: t.defaultLabel });
    expect(back.value).toBe("1");
    expect(within(back).getByRole("option", { name: t.defaultOption({ label: "Profile 1 · Work" }) })).toBeTruthy();
    fireEvent.change(back, { target: { value: "" } });
    await waitFor(() => {
      expect(rulesSent().default_profile).toBeNull();
    });
  });

  it("shows the live line and the recent activity the worker recorded", async () => {
    const switched = recorded("games_live", "get_auto_profile_state", 1);
    install({ view: switched });
    await openGames();
    expect(screen.getByRole("status").textContent).toContain(
      t.line.switched({ label: "Counter-Strike 2", profile: "Profile 2 · FPS" }),
    );
    expect(
      screen.getByText(t.recent.switched({ label: "Counter-Strike 2", from: "Profile 1 · Work", to: "Profile 2 · FPS" })),
    ).toBeTruthy();
    // The game rule holds the profile: it is not "Auto paused".
    expect(screen.getByRole("button", { name: t.pause })).toBeTruthy();
  });

  it("pauses for this run and says so", async () => {
    install({ view: LIVE_RULES, pause: recorded("games_paused", "pause_auto_profile") });
    await openGames();
    fireEvent.click(screen.getByRole("button", { name: t.pause }));
    await waitFor(() => {
      expect(named("pause_auto_profile").map((call) => call.args)).toEqual([{ paused: true }]);
    });
    expect(await screen.findByRole("button", { name: t.resume })).toBeTruthy();
    expect(screen.getByRole("status").textContent).toContain(t.line.paused({ reason: t.pauseReasons.user }));
  });

  it("says why the running list and detection can't be used without program access", async () => {
    install({ view: LIVE_RULES });
    await openGames();
    fireEvent.click(screen.getByRole("button", { name: t.rules.add }));
    const add = within(dialog());
    expect(add.getByRole<HTMLButtonElement>("button", { name: t.add.runningShow }).disabled).toBe(true);
    expect(add.getByRole<HTMLButtonElement>("button", { name: t.add.detect }).disabled).toBe(true);
    expect(add.getAllByText(t.add.unavailable).length).toBeGreaterThan(0);
    // Browse and a typed name still work.
    expect(add.getByRole<HTMLButtonElement>("button", { name: t.add.browse }).disabled).toBe(false);
  });

  it("has every text in Turkish too", async () => {
    setLang("tr");
    install({ view: recorded("games_live", "get_auto_profile_state", 1) });
    window.location.hash = "#/settings?tab=games";
    render(<SettingsPage />);
    const tr = DICTIONARIES.tr.settings.games;
    expect(await screen.findByRole("switch", { name: tr.toggle })).toBeTruthy();
    expect(screen.getByRole("status").textContent).toContain(
      tr.line.switched({ label: "Counter-Strike 2", profile: "Profil 2 · FPS" }),
    );
  });
});
