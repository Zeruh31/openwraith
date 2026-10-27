// The Lighting page's pure logic: control visibility per effect, the draft,
// the intent (only changed fields the effect uses), the summary, the apply
// block reasons and the draft store's profile basis.

import { beforeEach, describe, expect, it } from "vitest";

import type { DeviceStatus, LightState, LightValue } from "../../ipc/bindings";
import { discardDrafts, draftPages } from "../../state/drafts";
import { INITIAL_STATUS } from "../../state/status";
import { basisOf, getDraftState, rebase, resetDraftStore, seedDraft, setDraft } from "./draftStore";
import {
  applyBlock,
  buildIntent,
  controlsFor,
  draftFromPreview,
  EMPTY_DRAFT,
  hexId,
  intentChanges,
  isOffered,
  modeView,
  offeredModes,
  previewOpacity,
  shownLight,
  withField,
  type LightDraft,
} from "./model";

const CURRENT: LightValue = {
  mode: { id: 10, key: "custom" },
  brightness: 4,
  speed: 3,
  color: "#ff0000",
  full_color: true,
  power_on: true,
};

const LIGHT: LightState = {
  current: CURRENT,
  modes: [
    { id: 2, key: "wave" },
    { id: 1, key: "breath" },
    { id: 0, key: "static" },
    { id: 12, key: "speed_respond" },
    { id: 100, key: "music_rhythm" },
    { id: 10, key: "custom" },
  ],
  max_brightness: 4,
  max_speed: 4,
  editable: true,
  disabled: null,
};

const READY: DeviceStatus = {
  ...INITIAL_STATUS,
  revision: 5,
  generation: 1,
  connection: { state: "ready" },
  active_profile: 1,
  write_gate: { allowed: true, reasons: [] },
  snapshot_revision: 1,
};

function draft(fields: Partial<LightDraft>): LightDraft {
  return { ...EMPTY_DRAFT, ...fields };
}

describe("controls per effect", () => {
  it("hides the colour for Custom and the speed for Static, Custom and Speed response", () => {
    expect(controlsFor("wave")).toEqual({ color: true, speed: true, fullColor: true });
    expect(controlsFor("static")).toEqual({ color: true, speed: false, fullColor: false });
    expect(controlsFor("custom")).toEqual({ color: false, speed: false, fullColor: false });
    expect(controlsFor("speed_respond")).toEqual({ color: true, speed: false, fullColor: true });
  });

  it("never offers music rhythm", () => {
    expect(offeredModes(LIGHT).map((m) => m.key)).toEqual(["wave", "breath", "static", "speed_respond", "custom"]);
    expect(isOffered(LIGHT, 100)).toBe(false);
    expect(isOffered(LIGHT, 0)).toBe(true);
  });

  it("names unlisted modes as unknown with their id", () => {
    expect(modeView(LIGHT, 5)).toEqual({ id: 5, key: "unknown" });
    expect(modeView(LIGHT, 2)).toEqual({ id: 2, key: "wave" });
    expect(hexId(5)).toBe("0x05");
    expect(hexId(16)).toBe("0x10");
  });
});

describe("draft", () => {
  it("stores a value equal to the keyboard's as no change", () => {
    let d = withField(EMPTY_DRAFT, CURRENT, "brightness", 2);
    expect(d.brightness).toBe(2);
    d = withField(d, CURRENT, "brightness", 4);
    expect(d.brightness).toBeNull();
    expect(withField(EMPTY_DRAFT, CURRENT, "mode", 10).mode).toBeNull();
    expect(withField(EMPTY_DRAFT, CURRENT, "color", "#FF0000").color).toBeNull();
    expect(withField(EMPTY_DRAFT, CURRENT, "color", "#00FF00").color).toBe("#00ff00");
  });

  it("shows the draft over the keyboard's values", () => {
    const shown = shownLight(LIGHT, CURRENT, draft({ mode: 2, speed: 1 }));
    expect(shown.mode).toEqual({ id: 2, key: "wave" });
    expect(shown.speed).toBe(1);
    expect(shown.color).toBe("#ff0000");
    expect(shown.controls.speed).toBe(true);
    expect(shown.needsEffect).toBe(false);
  });

  it("asks for an effect when the keyboard's effect is unknown", () => {
    const unknown: LightState = { ...LIGHT, current: { ...CURRENT, mode: { id: 5, key: "unknown" } } };
    const current = unknown.current ?? CURRENT;
    expect(shownLight(unknown, current, EMPTY_DRAFT).needsEffect).toBe(true);
    expect(shownLight(unknown, current, draft({ mode: 0 })).needsEffect).toBe(false);
  });
});

describe("intent", () => {
  it("is null without changes", () => {
    expect(buildIntent(LIGHT, EMPTY_DRAFT, 1)).toBeNull();
    expect(buildIntent(LIGHT, draft({ brightness: 4 }), 1)).toBeNull();
  });

  it("sends only the changed fields with the snapshot revision", () => {
    expect(buildIntent(LIGHT, draft({ mode: 0, color: "#0000ff" }), 7)).toEqual({
      mode: 0,
      color: "#0000ff",
      brightness: null,
      speed: null,
      based_on: 7,
    });
    expect(buildIntent(LIGHT, draft({ brightness: 2 }), 1)).toEqual({
      mode: null,
      color: null,
      brightness: 2,
      speed: null,
      based_on: 1,
    });
  });

  it("leaves out fields the chosen effect doesn't use", () => {
    // Custom (the keyboard's effect) has no colour and no speed.
    expect(buildIntent(LIGHT, draft({ color: "#00ff00", speed: 1 }), 1)).toBeNull();
    // Static: colour yes, speed no.
    expect(buildIntent(LIGHT, draft({ mode: 0, color: "#00ff00", speed: 1 }), 1)).toEqual({
      mode: 0,
      color: "#00ff00",
      brightness: null,
      speed: null,
      based_on: 1,
    });
    // Wave uses both.
    expect(buildIntent(LIGHT, draft({ mode: 2, color: "#00ff00", speed: 1 }), 1)).toEqual({
      mode: 2,
      color: "#00ff00",
      brightness: null,
      speed: 1,
      based_on: 1,
    });
    // Speed response: colour yes, speed no.
    expect(buildIntent(LIGHT, draft({ mode: 12, speed: 0 }), 1)?.speed).toBeNull();
  });

  it("is null when the light wasn't read", () => {
    expect(buildIntent({ ...LIGHT, current: null }, draft({ brightness: 1 }), 1)).toBeNull();
  });

  it("lists the changes for the apply bar", () => {
    const intent = buildIntent(LIGHT, draft({ mode: 2, color: "#0000ff", brightness: 1, speed: 0 }), 1);
    if (intent === null) throw new Error("expected an intent");
    expect(intentChanges(LIGHT, CURRENT, intent)).toEqual([
      { field: "mode", from: CURRENT.mode, to: { id: 2, key: "wave" } },
      { field: "color", from: "#ff0000", to: "#0000ff" },
      { field: "brightness", from: 4, to: 1 },
      { field: "speed", from: 3, to: 0 },
    ]);
  });
});

describe("apply block", () => {
  const shown = shownLight(LIGHT, CURRENT, draft({ brightness: 2 }));

  it("allows writing when ready, idle, fresh and editable", () => {
    expect(applyBlock(READY, false, false, LIGHT, shown)).toBeNull();
  });

  it("names why writing is blocked", () => {
    const lost: DeviceStatus = {
      ...READY,
      connection: { state: "lost", error: { kind: "disconnected", detail: null, message: "x" } },
      write_gate: { allowed: false, reasons: ["not_connected"] },
    };
    expect(applyBlock(lost, false, false, LIGHT, shown)).toEqual({ kind: "gate", reason: "not_connected" });
    expect(applyBlock(READY, true, false, LIGHT, shown)).toEqual({ kind: "busy" });
    expect(applyBlock(READY, false, true, LIGHT, shown)).toEqual({ kind: "gate", reason: "snapshot_stale" });
    const paused: DeviceStatus = { ...READY, write_gate: { allowed: false, reasons: ["safety_dir_unavailable"] } };
    expect(applyBlock(paused, false, false, LIGHT, shown)).toEqual({ kind: "gate", reason: "safety_dir_unavailable" });
    expect(applyBlock(READY, false, false, { ...LIGHT, editable: false, disabled: "limits_unknown" }, shown)).toEqual({
      kind: "not_editable",
    });
    const unknown = shownLight(LIGHT, { ...CURRENT, mode: { id: 5, key: "unknown" } }, EMPTY_DRAFT);
    expect(applyBlock(READY, false, false, LIGHT, unknown)).toEqual({ kind: "choose_effect" });
  });
});

describe("preview helpers", () => {
  it("maps brightness to a visible intensity", () => {
    expect(previewOpacity(0, 4)).toBe(0.12);
    expect(previewOpacity(4, 4)).toBe(1);
    expect(previewOpacity(2, 4)).toBe(0.56);
    expect(previewOpacity(9, 4)).toBe(1);
  });

  it("reads a preset draft and ignores invalid fields", () => {
    expect(draftFromPreview(undefined)).toBe(EMPTY_DRAFT);
    expect(draftFromPreview({ draft: { mode: 0, color: "#0000ff" } })).toEqual(draft({ mode: 0, color: "#0000ff" }));
    expect(draftFromPreview({ draft: { mode: "x", color: "blue", brightness: -1, speed: 1.5 } })).toEqual(EMPTY_DRAFT);
  });
});

describe("draft store", () => {
  beforeEach(() => {
    resetDraftStore();
  });

  it("keeps a draft made before the first snapshot and drops it on another profile", () => {
    seedDraft(draft({ brightness: 2 }));
    expect(rebase("1:1")).toBe(false);
    expect(getDraftState()).toEqual({ basis: "1:1", draft: draft({ brightness: 2 }) });
    expect(rebase("1:1")).toBe(false);
    expect(rebase("1:2")).toBe(true);
    expect(getDraftState()).toEqual({ basis: "1:2", draft: EMPTY_DRAFT });
    expect(rebase("2:2")).toBe(false);
  });

  it("doesn't build on a draft made for another profile", () => {
    setDraft("1:1", (d) => ({ ...d, brightness: 1 }));
    setDraft("1:2", (d) => ({ ...d, speed: 2 }));
    expect(getDraftState()).toEqual({ basis: "1:2", draft: draft({ speed: 2 }) });
  });

  it("is known to the shell's draft registry while it exists, on the page or not", () => {
    expect(draftPages()).not.toContain("lighting");
    setDraft("1:1", (d) => ({ ...d, brightness: 1 }));
    expect(draftPages()).toContain("lighting");
    // The shell discards drafts (a confirmed profile switch): the draft is cleared.
    discardDrafts(["lighting"]);
    expect(getDraftState().draft).toBe(EMPTY_DRAFT);
    expect(draftPages()).not.toContain("lighting");
  });

  it("normalises an all-unchanged draft to the empty draft", () => {
    setDraft("1:1", () => draft({}));
    expect(getDraftState().draft).toBe(EMPTY_DRAFT);
    expect(basisOf({ generation: 3, profile: 2 })).toBe("3:2");
    expect(basisOf(null)).toBeNull();
  });
});
