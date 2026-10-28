// The SOCD tab's logic on the recorded simulator states (socd_ready,
// socd_ready_pairs): the site's rules for pairs, quick pairs, the mode
// evidence from the state, and the intents the recorded plans were made from.

import { describe, expect, it } from "vitest";

import type { SocdEditIntent, SocdStateView } from "../../../ipc/bindings";
import { FIXTURES } from "../../../preview/registry";
import {
  buildSocdIntent,
  canQuickAdd,
  checkPairs,
  EMPTY_SOCD,
  isSocdDirty,
  modeEvidence,
  normalize,
  offeredKeys,
  pairsOf,
  rowsOf,
  samePairs,
  usedKeys,
  type SocdPairDraft,
} from "./model";

const W = 46;
const A = 68;
const S = 69;
const D = 70;
const Q = 45;
const E = 47;

function state(id: string): SocdStateView {
  const call = FIXTURES[id]?.calls.find((c) => c.command === "get_socd_state");
  if (call?.command !== "get_socd_state" || call.ok === null) throw new Error(`no SOCD state in ${id}`);
  return structuredClone(call.ok);
}

function intent(id: string): SocdEditIntent {
  const call = FIXTURES[id]?.calls.find((c) => c.command === "prepare_socd_edit");
  if (call?.command !== "prepare_socd_edit") throw new Error(`no prepare in ${id}`);
  return call.args.intent;
}

const row = (id: number, key1: number | null, key2: number | null, mode = 1): SocdPairDraft => ({ id, key1, key2, mode });

describe("the pairs of the keyboard", () => {
  it("are shown as rows by key index", () => {
    const ready = state("socd_ready_pairs");
    expect(pairsOf(ready)).toEqual([row(1, A, D, 1), row(2, W, S, 2), row(3, 123, 125, 4)]);
    expect(samePairs(pairsOf(ready), ready.pairs)).toBe(true);
    expect(samePairs([row(1, A, D, 4)], ready.pairs)).toBe(false);
    expect(rowsOf(EMPTY_SOCD, ready)).toHaveLength(3);
  });

  it("carry the library's evidence per mode: 1 and 4 confirmed, 2 and 3 not yet verified", () => {
    const ready = state("socd_ready");
    expect(modeEvidence(ready, 1)).toEqual({ status: "confirmed" });
    expect(modeEvidence(ready, 2)?.status).toBe("unverified");
    expect(modeEvidence(ready, 3)?.status).toBe("unverified");
    expect(modeEvidence(ready, 4)).toEqual({ status: "confirmed" });
    expect(modeEvidence(ready, 9)).toBeNull();
  });

  it("offer every layout key with a HID usage, never the knob", () => {
    const offered = offeredKeys(state("socd_ready"));
    expect(offered.has(A)).toBe(true);
    expect(offered.has(15)).toBe(false);
  });
});

describe("the site's rules", () => {
  it("need both keys, two different keys and a key in one pair only", () => {
    const check = checkPairs([row(1, A, D), row(2, A, A), row(3, W, null), row(4, D, S)], 20);
    expect(check.ok).toBe(false);
    expect(check.problems.get(1)).toBeUndefined();
    expect(check.problems.get(2)).toEqual([
      { kind: "same_key", key: A },
      { kind: "duplicate", key: A, pair: 1 },
    ]);
    expect(check.problems.get(3)).toEqual([{ kind: "incomplete" }]);
    expect(check.problems.get(4)).toEqual([{ kind: "duplicate", key: D, pair: 1 }]);
  });

  it("allow at most 20 pairs and modes 1 to 4", () => {
    const many = Array.from({ length: 21 }, (_, i) => row(i + 1, 200 + i * 2, 201 + i * 2));
    expect(checkPairs(many, 20).tooMany).toBe(true);
    expect(checkPairs([row(1, A, D, 5)], 20).problems.get(1)).toEqual([{ kind: "mode" }]);
    expect(checkPairs([row(1, A, D, 4)], 20).ok).toBe(true);
  });

  it("offer a quick pair only when both keys are free and there is room", () => {
    const offered = offeredKeys(state("socd_ready"));
    expect(canQuickAdd([], "ad", 20, offered)).toBe(true);
    expect(canQuickAdd([row(1, A, Q)], "ad", 20, offered)).toBe(false);
    expect(canQuickAdd([row(1, Q, E)], "ws", 1, offered)).toBe(false);
    expect(usedKeys([row(1, A, D), row(2, W, S)], 2)).toEqual(new Map([[A, 1], [D, 1]]));
  });
});

describe("the intent", () => {
  it("is the recorded one of each plan", () => {
    const ready = state("socd_ready");
    expect(
      buildSocdIntent(ready, { power: true, pairs: [row(1, A, D, 1)], allowNotUndoable: false }, false),
    ).toEqual(intent("socd_apply_ok"));
    expect(
      buildSocdIntent(
        ready,
        { power: true, pairs: [row(1, A, D, 1), row(2, W, S, 2), row(3, Q, E, 3)], allowNotUndoable: false },
        false,
      ),
    ).toEqual(intent("socd_preview"));
    expect(buildSocdIntent(ready, { power: null, pairs: [row(1, A, D, 4)], allowNotUndoable: false }, false)).toEqual(
      intent("socd_toggled"),
    );
  });

  it("drops what equals the keyboard and refuses rows that break a rule", () => {
    const ready = state("socd_ready_pairs");
    const same = normalize({ power: true, pairs: pairsOf(ready), allowNotUndoable: true }, ready);
    expect(same).toEqual(EMPTY_SOCD);
    expect(isSocdDirty(same)).toBe(false);
    expect(buildSocdIntent(ready, same, false)).toBeNull();
    expect(buildSocdIntent(ready, { power: null, pairs: [row(1, A, A)], allowNotUndoable: false }, false)).toBeNull();
    expect(buildSocdIntent(ready, { power: false, pairs: null, allowNotUndoable: false }, true)).toEqual({
      power: false,
      pairs: null,
      allow_not_undoable: false,
      check_persistence: true,
      based_on: 1,
    });
  });

  it("asks for the not-undoable confirmation only for pairs the site can't write", () => {
    const odd = { ...state("socd_ready_pairs"), pairs: [], pairs_writable: false };
    expect(rowsOf(EMPTY_SOCD, odd)).toEqual([]);
    const replaced = normalize({ power: null, pairs: [], allowNotUndoable: true }, odd);
    expect(replaced.pairs).toEqual([]);
    expect(buildSocdIntent(odd, replaced, false)).toMatchObject({ pairs: [], allow_not_undoable: true });
  });
});
