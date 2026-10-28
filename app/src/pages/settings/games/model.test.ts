// The pure part of Settings > Games: intents built from the recorded views
// (programs of existing rules kept by id, new ones by token or typed name,
// the whole list each time), file-name checks, and every live line and
// activity entry the core's tests recorded, in both languages.

import { describe, expect, it } from "vitest";

import { DICTIONARIES, type Dict } from "../../../i18n";
import type { AutoEventView, AutoLineView, AutoProfileStateView } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import {
  eventText,
  exeNameProblem,
  keepIntent,
  labelFromExe,
  lineText,
  withDefaultProfile,
  withEnabled,
  withRuleAdded,
  withRuleChanged,
  withRuleRemoved,
} from "./model";

/** Every `AutoProfileStateView` the fixtures recorded. */
function recordedViews(): AutoProfileStateView[] {
  const views: AutoProfileStateView[] = [];
  for (const fixture of Object.values(FIXTURES)) {
    for (const call of fixture.calls) {
      if (
        (call.command === "get_auto_profile_state" ||
          call.command === "set_auto_profile_rules" ||
          call.command === "pause_auto_profile") &&
        call.ok !== null
      ) {
        views.push(call.ok);
      }
    }
  }
  return views;
}

function view(id: string, command: "set_auto_profile_rules" | "get_auto_profile_state"): AutoProfileStateView {
  const call = FIXTURES[id]?.calls.find((c) => c.command === command && c.ok !== null);
  if (call === undefined || call.ok === null) throw new Error(`no ${command} in ${id}`);
  return structuredClone(call.ok) as AutoProfileStateView;
}

const profile = (dict: Dict) => (n: number) => dict.common.profile({ n });

describe("the rules intents", () => {
  const live = view("games_live", "set_auto_profile_rules");

  it("send every stored rule again with its program kept", () => {
    expect(keepIntent(live)).toEqual({
      enabled: true,
      default_profile: 1,
      rules: live.rules.map((rule) => ({
        id: rule.id,
        label: rule.label,
        program: { source: "keep" },
        profile: rule.profile,
        enabled: rule.enabled,
      })),
    });
  });

  it("change one thing at a time", () => {
    expect(withEnabled(live, false).enabled).toBe(false);
    expect(withEnabled(live, false).rules).toEqual(keepIntent(live).rules);
    expect(withDefaultProfile(live, null).default_profile).toBeNull();
    const changed = withRuleChanged(live, "r-2", { profile: 4, enabled: false });
    expect(changed.rules.find((rule) => rule.id === "r-2")).toMatchObject({ profile: 4, enabled: false, program: { source: "keep" } });
    expect(changed.rules.filter((rule) => rule.id !== "r-2")).toEqual(keepIntent(live).rules.filter((rule) => rule.id !== "r-2"));
    expect(withRuleRemoved(live, "r-2").rules.map((rule) => rule.id)).toEqual(["r-1", "r-3"]);
  });

  it("add a new rule by token or typed name, on, without an id", () => {
    const added = withRuleAdded(live, { label: "  Valorant ", program: { source: "picked", token: "g-cd7ad687788f5ad5" }, profile: 2 });
    expect(added.rules).toHaveLength(live.rules.length + 1);
    expect(added.rules[added.rules.length - 1]).toEqual({
      id: null,
      label: "Valorant",
      program: { source: "picked", token: "g-cd7ad687788f5ad5" },
      profile: 2,
      enabled: true,
    });
    // The worker recorded the same shape for a typed name.
    const settingsCall = FIXTURES["games_settings"]?.calls.filter((c) => c.command === "set_auto_profile_rules")[1];
    if (settingsCall?.command !== "set_auto_profile_rules") throw new Error("no recorded typed rule");
    const typed = settingsCall.args.rules.rules[0];
    if (typed === undefined) throw new Error("no typed rule");
    const off = view("games_settings", "get_auto_profile_state");
    expect(
      withRuleAdded(off, { label: typed.label, program: typed.program as { source: "name"; exe: string }, profile: typed.profile })
        .rules,
    ).toEqual([typed]);
  });

  it("name a game from its program and refuse folders in typed names", () => {
    expect(labelFromExe("VALORANT.exe")).toBe("VALORANT");
    expect(labelFromExe("cs2.EXE")).toBe("cs2");
    expect(labelFromExe("x".repeat(80) + ".exe")).toHaveLength(64);
    expect(exeNameProblem("  ")).toBe("empty");
    expect(exeNameProblem("C:\\Games\\x.exe")).toBe("folder");
    expect(exeNameProblem("games/x.exe")).toBe("folder");
    expect(exeNameProblem("x.exe")).toBeNull();
  });
});

describe("the live line and the activity", () => {
  const views = recordedViews();
  const lines: AutoLineView[] = views.map((v) => v.line);
  const events: AutoEventView[] = views.flatMap((v) => v.recent.map((entry) => entry.event));

  it("cover the recorded kinds", () => {
    const lineKinds = new Set(lines.map((line) => (line.line === "game_in_front" ? `in_front:${line.state}` : line.line)));
    for (const kind of ["off", "watching", "paused", "waiting", "in_front:dwelling", "in_front:switched", "in_front:skipped"]) {
      expect(lineKinds.has(kind), kind).toBe(true);
    }
    const eventKinds = new Set<string>(events.map((event) => event.event));
    for (const kind of ["switched", "deferred", "skipped", "paused", "resumed"]) {
      expect(eventKinds.has(kind), kind).toBe(true);
    }
  });

  it("have words for every recorded line and entry in both languages", () => {
    for (const lang of ["en", "tr"] as const) {
      const dict = DICTIONARIES[lang];
      for (const line of [...lines, { line: "no_rules" } as const]) {
        const text = lineText(line, dict, profile(dict));
        expect(text, JSON.stringify(line)).not.toMatch(/undefined|null|\[object/);
        expect(text.length).toBeGreaterThan(5);
      }
      for (const event of [
        ...events,
        { event: "overridden", rule_id: "r-1", label: "Counter-Strike 2", profile: 3 } as const,
        { event: "overridden", rule_id: null, label: null, profile: 3 } as const,
      ]) {
        const text = eventText(event, dict, profile(dict));
        expect(text, JSON.stringify(event)).not.toMatch(/undefined|null|\[object/);
      }
    }
  });

  it("say what waits and why, and name the switch back to the default profile", () => {
    const en = DICTIONARIES.en;
    const waiting = lines.find((line) => line.line === "waiting");
    if (waiting?.line !== "waiting") throw new Error("no recorded waiting line");
    expect(lineText(waiting, en, profile(en))).toBe(
      en.settings.games.line.waiting({
        label: waiting.wait.label ?? "",
        profile: en.common.profile({ n: waiting.wait.profile }),
        reason: en.shell.automation.waitReasons[waiting.wait.reason],
      }),
    );
    const back = events.find((event) => event.event === "switched" && event.cause === "game_exited");
    if (back?.event !== "switched") throw new Error("no recorded switch back");
    expect(eventText(back, en, profile(en))).toBe(en.settings.games.recent.switchedBack({ to: en.common.profile({ n: back.to }) }));
  });
});
