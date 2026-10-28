// Settings > Games, the pure part: the intents that `set_auto_profile_rules`
// takes (it replaces every rule, so each change sends the whole list; the
// programs of existing rules are kept by id, new ones come by token or by a
// typed file name) and the status line and activity in words.

import type { Dict } from "../../../i18n";
import type {
  AutoEpisodeState,
  AutoEventView,
  AutoLineView,
  AutoPauseReason,
  AutoProfileRuleIntent,
  AutoProfileRuleView,
  AutoProfileRulesIntent,
  AutoProfileStateView,
  AutoWaitView,
  RuleProgramIntent,
} from "../../../ipc/bindings";

/** The longest game name the backend keeps. */
export const LABEL_MAX = 64;

/** A rule the user is adding: its program comes by token (picked) or by a typed file name. */
export interface NewRule {
  label: string;
  program: Extract<RuleProgramIntent, { source: "picked" } | { source: "name" }>;
  profile: number;
}

/** A change to one stored rule. */
export type RulePatch = Partial<Pick<AutoProfileRuleIntent, "profile" | "enabled">>;

/** A stored rule as the intent sends it again (its program kept). */
export function keepRule(rule: AutoProfileRuleView): AutoProfileRuleIntent {
  return { id: rule.id, label: rule.label, program: { source: "keep" }, profile: rule.profile, enabled: rule.enabled };
}

/** The intent that saves `view` unchanged. */
export function keepIntent(view: AutoProfileStateView): AutoProfileRulesIntent {
  return { enabled: view.enabled, default_profile: view.default_profile, rules: view.rules.map(keepRule) };
}

/** Turns the automatic profiles on or off. */
export function withEnabled(view: AutoProfileStateView, enabled: boolean): AutoProfileRulesIntent {
  return { ...keepIntent(view), enabled };
}

/** Sets the profile to go back to when a game closes (`null`: stay). */
export function withDefaultProfile(view: AutoProfileStateView, profile: number | null): AutoProfileRulesIntent {
  return { ...keepIntent(view), default_profile: profile };
}

/** Changes one rule (by id). */
export function withRuleChanged(view: AutoProfileStateView, id: string, patch: RulePatch): AutoProfileRulesIntent {
  const intent = keepIntent(view);
  return { ...intent, rules: intent.rules.map((rule) => (rule.id === id ? { ...rule, ...patch } : rule)) };
}

/** Removes one rule (by id). */
export function withRuleRemoved(view: AutoProfileStateView, id: string): AutoProfileRulesIntent {
  const intent = keepIntent(view);
  return { ...intent, rules: intent.rules.filter((rule) => rule.id !== id) };
}

/** Adds a rule (on; the backend gives it an id). */
export function withRuleAdded(view: AutoProfileStateView, rule: NewRule): AutoProfileRulesIntent {
  const intent = keepIntent(view);
  return {
    ...intent,
    rules: [
      ...intent.rules,
      { id: null, label: rule.label.trim(), program: rule.program, profile: rule.profile, enabled: true },
    ],
  };
}

/** A game's name from its program ("VALORANT.exe" -> "VALORANT"). */
export function labelFromExe(exe: string): string {
  return exe.replace(/\.exe$/i, "").slice(0, LABEL_MAX);
}

/**
 * A typed program name as the rule keeps it: trimmed, and `<name>.exe` when
 * it has no extension (the backend does the same; rules compare the whole
 * file name, so "VALORANT" alone would never match `VALORANT.exe`).
 */
export function typedProgramName(text: string): string {
  const name = text.trim();
  const dot = name.lastIndexOf(".");
  const hasExtension = dot > 0 && dot < name.length - 1;
  return name === "" || hasExtension ? name : `${name}.exe`;
}

/** Why a typed file name can't be used (`null`: it can). */
export function exeNameProblem(text: string): "empty" | "folder" | null {
  const name = text.trim();
  if (name === "") return "empty";
  if (/[\\/:]/.test(name)) return "folder";
  return null;
}

/** Why the game profiles are paused, completing "Paused: …". */
export function pauseReasonText(reason: AutoPauseReason, dict: Dict): string {
  return dict.settings.games.pauseReasons[reason.reason];
}

/** The game's name, or "A game" when the rule has none. */
function labelOf(label: string | null, dict: Dict): string {
  return label ?? dict.settings.games.recent.aGame;
}

function waitingText(wait: AutoWaitView, dict: Dict, profile: (n: number) => string): string {
  const line = dict.settings.games.line;
  const reason = dict.shell.automation.waitReasons[wait.reason];
  return wait.cause === "game_exited"
    ? line.waitingBack({ profile: profile(wait.profile), reason })
    : line.waiting({ label: labelOf(wait.label, dict), profile: profile(wait.profile), reason });
}

/** The live status line in words; `profile` names a profile ("Profile 2 · FPS"). */
export function lineText(line: AutoLineView, dict: Dict, profile: (n: number) => string): string {
  const t = dict.settings.games.line;
  switch (line.line) {
    case "off":
      return t.off;
    case "paused":
      return t.paused({ reason: pauseReasonText(line.reason, dict) });
    case "no_rules":
      return t.noRules;
    case "watching":
      return t.watching;
    case "game_in_front":
      return inFrontText(line.state, { label: line.label, profile: profile(line.profile) }, dict);
    case "waiting":
      return waitingText(line.wait, dict, profile);
  }
}

function inFrontText(state: AutoEpisodeState, words: { label: string; profile: string }, dict: Dict): string {
  const t = dict.settings.games.line;
  switch (state) {
    case "dwelling":
      return t.dwelling(words);
    case "switched":
      return t.switched(words);
    case "overridden":
      return t.overridden(words);
    case "skipped":
      return t.skipped(words);
  }
}

/** One entry of the recent activity in words. */
export function eventText(event: AutoEventView, dict: Dict, profile: (n: number) => string): string {
  const t = dict.settings.games.recent;
  switch (event.event) {
    case "switched":
      if (event.cause === "game_exited") return t.switchedBack({ to: profile(event.to) });
      return event.from === null
        ? t.switchedTo({ label: labelOf(event.label, dict), to: profile(event.to) })
        : t.switched({ label: labelOf(event.label, dict), from: profile(event.from), to: profile(event.to) });
    case "deferred": {
      const reason = dict.shell.automation.waitReasons[event.reason];
      return event.cause === "game_exited"
        ? t.deferredBack({ profile: profile(event.profile), reason })
        : t.deferred({ label: labelOf(event.label, dict), profile: profile(event.profile), reason });
    }
    case "skipped": {
      const reason = dict.shell.automation.waitReasons[event.reason];
      return event.cause === "game_exited"
        ? t.skippedBack({ profile: profile(event.profile), reason })
        : t.skipped({ label: labelOf(event.label, dict), profile: profile(event.profile), reason });
    }
    case "overridden":
      return event.label === null
        ? t.overriddenNoGame({ profile: profile(event.profile) })
        : t.overridden({ label: event.label, profile: profile(event.profile) });
    case "paused":
      return t.paused({ reason: pauseReasonText(event.reason, dict) });
    case "resumed":
      return t.resumed;
  }
}
