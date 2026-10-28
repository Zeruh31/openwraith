// Keys > Advanced keys: the tab's logic as pure functions.
//
// - The DKS step grammar comes from the generated catalogue
//   (`ACTION_CATALOG.dks_rows`, every grammar-valid row with the library's
//   evidence): which step a cell may take is derived from those rows, never
//   from a list kept here.
// - The draft is a list of advanced-key changes (new, edited, assigned,
//   deleted); `changesOf` turns it into the library's change intents, one
//   change per draft item, in the order they were made.
// - Sentences say in words what a definition does (the live sentence of the
//   editors and the cards).
//
// Nothing here talks to the keyboard.

import { ACTION_CATALOG, bindingEvidence, combineEvidence, usageName } from "../../../components/actions";
import type { Dict, Lang } from "../../../i18n";
import type {
  ActionCatalog,
  AdvancedDefView,
  AdvancedSlotStateView,
  BuiltinAckIntent,
  DksRowRuleView,
  DksRowView,
  DksStepView,
  EvidenceView,
  KeyAssignmentsView,
  KeymapChangeIntent,
  KeysState,
  LayerView,
} from "../../../ipc/bindings";
import { change, neededAcks } from "../shared";

/** The kinds of advanced keys, in the order the tiles show them. */
export type AdvancedType = AdvancedDefView["type"];

/** The tile order. */
export const ADVANCED_TYPES: readonly AdvancedType[] = ["dks", "mod_tap", "toggle"];

/** Every step value, in the order a cell cycles through them. */
export const DKS_STEPS: readonly DksStepView[] = ["none", "tap", "hold", "hold_again"];

/** The four stages of a DKS row (0-based). */
export const DKS_STAGES: readonly number[] = [0, 1, 2, 3];

/** The number of rows (keys) of a DKS. */
export const DKS_ROWS = 4;

/** The first point of a new DKS in 0.1 mm (the library's UI default, 1.5 mm). */
export const DKS_DEFAULT_POINT = 15;

/** The usage used when a key has no usage of its own (A). */
const FALLBACK_USAGE = 0x04;

/** Left Ctrl, the hold key of a new Tap-or-hold. */
const LEFT_CTRL = 0xe0;

// ---------- the DKS grammar (from the catalogue) ----------

function sameSteps(a: readonly DksStepView[], b: readonly DksStepView[]): boolean {
  return a.length === b.length && a.every((step, index) => step === b[index]);
}

/** The catalogue's rule for a row's steps; `undefined` when the keyboard doesn't accept them (or all are Nothing). */
export function rowRule(steps: readonly DksStepView[], catalog: ActionCatalog = ACTION_CATALOG): DksRowRuleView | undefined {
  return catalog.dks_rows.find((rule) => sameSteps(rule.steps, steps));
}

/**
 * The steps a stage may take after the earlier stages: Nothing always (a row
 * that does nothing anywhere is reported, not prevented), plus every step a
 * grammar-valid row with the same earlier steps has at that stage.
 */
export function allowedSteps(
  steps: readonly DksStepView[],
  stage: number,
  catalog: ActionCatalog = ACTION_CATALOG,
): DksStepView[] {
  const prefix = steps.slice(0, stage);
  const allowed = new Set<DksStepView>(["none"]);
  for (const rule of catalog.dks_rows) {
    const step = rule.steps[stage];
    if (step !== undefined && sameSteps(rule.steps.slice(0, stage), prefix)) allowed.add(step);
  }
  return DKS_STEPS.filter((step) => allowed.has(step));
}

/**
 * Sets one stage and repairs the later stages that the change made invalid
 * ("let go and press again" without a hold before it becomes a hold, or
 * Nothing).
 */
export function setStep(
  steps: readonly DksStepView[],
  stage: number,
  step: DksStepView,
  catalog: ActionCatalog = ACTION_CATALOG,
): DksStepView[] {
  const next = [...steps];
  next[stage] = step;
  for (let later = stage + 1; later < next.length; later += 1) {
    const allowed = allowedSteps(next, later, catalog);
    const current = next[later] ?? "none";
    if (allowed.includes(current)) continue;
    next[later] = current === "hold_again" && allowed.includes("hold") ? "hold" : "none";
  }
  return next;
}

/** The step a cell takes next (Space, Enter or a click): the next allowed one, wrapping around. */
export function cycleStep(steps: readonly DksStepView[], stage: number, catalog: ActionCatalog = ACTION_CATALOG): DksStepView {
  const allowed = allowedSteps(steps, stage, catalog);
  const current = steps[stage] ?? "none";
  const index = allowed.indexOf(current);
  return allowed[(index + 1) % allowed.length] ?? "none";
}

/** What is wrong with a row: nothing, no step at all, or a combination the keyboard doesn't take. */
export type RowProblem = "empty" | "invalid" | null;

/** Checks a row against the catalogue's rows. */
export function rowProblem(steps: readonly DksStepView[], catalog: ActionCatalog = ACTION_CATALOG): RowProblem {
  if (steps.every((step) => step === "none")) return "empty";
  return rowRule(steps, catalog) === undefined ? "invalid" : null;
}

/** The rows that are used, with their position (0-3). */
export function usedRows(rows: readonly (DksRowView | null)[]): { position: number; row: DksRowView }[] {
  const used: { position: number; row: DksRowView }[] = [];
  rows.forEach((row, position) => {
    if (row !== null) used.push({ position, row });
  });
  return used;
}

/** Four row positions (a definition from the keyboard always has four). */
export function fourRows(rows: readonly (DksRowView | null)[]): (DksRowView | null)[] {
  return Array.from({ length: DKS_ROWS }, (_, index) => rows[index] ?? null);
}

// ---------- evidence (from the catalogue and the DTOs only) ----------

/** The evidence of a DKS: every row's rule; `null` when a row isn't valid. */
export function dksEvidence(
  rows: readonly (DksRowView | null)[],
  catalog: ActionCatalog = ACTION_CATALOG,
): EvidenceView | null {
  const parts: EvidenceView[] = [];
  for (const { row } of usedRows(rows)) {
    const rule = rowRule(row.steps, catalog);
    if (rule === undefined) return null;
    parts.push(rule.evidence);
  }
  return combineEvidence(...parts);
}

/** The evidence of a Tap-or-hold hold time: the tested times are confirmed, any other carries the library's note. */
export function holdTimeEvidence(holdMs: number, catalog: ActionCatalog = ACTION_CATALOG): EvidenceView {
  return catalog.mod_tap.confirmed_ms.includes(holdMs) ? { status: "confirmed" } : catalog.mod_tap.other_evidence;
}

/**
 * The evidence of a definition being drafted: the DKS rows' rules, the
 * Tap-or-hold time; `null` when the catalogue has no rule for it (Toggle) or
 * a DKS row isn't valid.
 */
export function defEvidence(def: AdvancedDefView, catalog: ActionCatalog = ACTION_CATALOG): EvidenceView | null {
  switch (def.type) {
    case "dks":
      return dksEvidence(def.rows, catalog);
    case "mod_tap":
      return holdTimeEvidence(def.hold_ms, catalog);
    case "toggle":
      return null;
  }
}

/** The evidence of binding an advanced key on a layer (the catalogue's binding rule). */
export function layerBindingEvidence(layer: LayerView, catalog: ActionCatalog = ACTION_CATALOG): EvidenceView | null {
  return bindingEvidence("advanced", layer, catalog);
}

// ---------- keys ----------

/** The HID usage a layout key sends by itself (`null` when it has none). */
export function ownUsage(index: number, catalog: ActionCatalog = ACTION_CATALOG): number | null {
  const id = catalog.layout_keys.find((key) => key.index === index)?.id;
  if (id?.startsWith("key:") !== true) return null;
  const usage = Number.parseInt(id.slice(4), 16);
  return Number.isNaN(usage) ? null : usage;
}

/** Why a key can't take an advanced key on a layer, or `null` when it can. */
export function targetBlock(key: KeyAssignmentsView, layer: LayerView): "fn_key" | "knob" | "fn_locked" | "not_assignable" | null {
  if (key.kind === "fn_key") return "fn_key";
  if (key.kind === "knob") return "knob";
  if (layer === "fn" && key.fn_locked) return "fn_locked";
  const assignable = layer === "base" ? key.assignable_base : key.assignable_fn;
  return assignable ? null : "not_assignable";
}

// ---------- new definitions ----------

/** A new definition of a type for a key (its own usage where it has one). */
export function newDef(
  type: AdvancedType,
  usage: number | null,
  catalog: ActionCatalog = ACTION_CATALOG,
): AdvancedDefView {
  const key = usage ?? FALLBACK_USAGE;
  switch (type) {
    case "dks":
      return {
        type: "dks",
        first_point: DKS_DEFAULT_POINT,
        rows: [{ key, steps: ["tap", "none", "none", "none"] }, null, null, null],
      };
    case "mod_tap":
      return { type: "mod_tap", tap: key, hold: LEFT_CTRL, hold_ms: catalog.mod_tap.default_ms };
    case "toggle":
      return { type: "toggle", key };
  }
}

/** Whether two definitions are the same. */
export function sameDef(a: AdvancedDefView, b: AdvancedDefView): boolean {
  if (a.type === "dks" && b.type === "dks") {
    if (a.first_point !== b.first_point) return false;
    const left = fourRows(a.rows);
    const right = fourRows(b.rows);
    return left.every((row, index) => {
      const other = right[index] ?? null;
      if (row === null || other === null) return row === other;
      return row.key === other.key && sameSteps(row.steps, other.steps);
    });
  }
  if (a.type === "mod_tap" && b.type === "mod_tap") {
    return a.tap === b.tap && a.hold === b.hold && a.hold_ms === b.hold_ms;
  }
  if (a.type === "toggle" && b.type === "toggle") return a.key === b.key;
  return false;
}

/** Why a definition can't go into the draft yet (texts from the dictionary), or `null`. */
export function defProblem(
  def: AdvancedDefView,
  state: KeysState,
  dict: Dict,
  catalog: ActionCatalog = ACTION_CATALOG,
): string | null {
  const t = dict.keysAdvanced;
  switch (def.type) {
    case "dks": {
      const used = usedRows(def.rows);
      if (used.length === 0) return t.dks.needRow;
      for (const { row } of used) {
        const problem = rowProblem(row.steps, catalog);
        if (problem === "empty") return t.dks.rowEmpty({ key: usageName(row.key, dict) });
        if (problem === "invalid") return t.dks.rowInvalid({ key: usageName(row.key, dict) });
      }
      const max = state.limits.dks_first_point_max;
      if (def.first_point < 1 || (max !== null && def.first_point > max)) return t.dks.pointRange;
      return null;
    }
    case "mod_tap": {
      const { mt_hold_ms_min: min, mt_hold_ms_max: max, mt_hold_ms_step: step } = state.limits;
      if (def.hold_ms < min || def.hold_ms > max || def.hold_ms % step !== 0) return t.mt.timeRange;
      return null;
    }
    case "toggle":
      return null;
  }
}

// ---------- quick starts ----------

/** A DKS quick start: the steps of its rows (keys are kept or filled in). */
export interface QuickStart {
  id: "twoDepths" | "pressLift" | "holdThenSwitch";
  steps: readonly (readonly DksStepView[])[];
}

/** The quick starts of the DKS editor. */
export const QUICK_STARTS: readonly QuickStart[] = [
  { id: "twoDepths", steps: [["tap", "none", "none", "none"], ["none", "tap", "none", "none"]] },
  { id: "pressLift", steps: [["tap", "none", "none", "none"], ["none", "none", "tap", "none"]] },
  { id: "holdThenSwitch", steps: [["hold", "none", "none", "none"], ["none", "hold", "none", "none"]] },
];

/**
 * The rows of a quick start: the keys of the rows already there, in order;
 * the first row falls back to `usage`, every later one to the row before it.
 */
export function applyQuickStart(
  quick: QuickStart,
  rows: readonly (DksRowView | null)[],
  usage: number | null,
): (DksRowView | null)[] {
  const keys = usedRows(rows).map(({ row }) => row.key);
  const next: (DksRowView | null)[] = [null, null, null, null];
  let previous = keys[0] ?? usage ?? FALLBACK_USAGE;
  quick.steps.forEach((steps, index) => {
    const key = keys[index] ?? previous;
    previous = key;
    next[index] = { key, steps: [...steps] };
  });
  return next;
}

/** The evidence of a quick start (its rows' rules). */
export function quickStartEvidence(quick: QuickStart, catalog: ActionCatalog = ACTION_CATALOG): EvidenceView | null {
  return dksEvidence(
    quick.steps.map((steps) => ({ key: FALLBACK_USAGE, steps: [...steps] })),
    catalog,
  );
}

// ---------- sentences ----------

/** A millimetre value given in 0.1 mm ("1.5 mm" / "1,5 mm"). */
export function tenthsMm(tenths: number, lang: Lang, dict: Dict): string {
  const text = (tenths / 10).toFixed(1);
  return dict.common.units.mm({ value: lang === "tr" ? text.replace(".", ",") : text });
}

/** What a DKS does, stage by stage ("Light press (1.5 mm): taps E. Full press: taps R."). */
export function dksSentence(
  def: Extract<AdvancedDefView, { type: "dks" }>,
  dict: Dict,
  lang: Lang,
  list: (items: readonly string[]) => string,
): string {
  const s = dict.keysAdvanced.dks.sentence;
  const point = tenthsMm(def.first_point, lang, dict);
  const stages = [s.stage1({ mm: point }), s.stage2, s.stage3, s.stage4({ mm: point })];
  const lines: string[] = [];
  DKS_STAGES.forEach((stage) => {
    const actions: string[] = [];
    for (const { row } of usedRows(def.rows)) {
      const step = row.steps[stage] ?? "none";
      if (step === "none") continue;
      actions.push(s[step]({ key: usageName(row.key, dict) }));
    }
    const name = stages[stage];
    if (actions.length > 0 && name !== undefined) lines.push(s.line({ stage: name, actions: list(actions) }));
  });
  return lines.length === 0 ? s.nothing : lines.join(" ");
}

/** What a definition does, in one or two sentences. */
export function defSentence(
  def: AdvancedDefView,
  dict: Dict,
  lang: Lang,
  list: (items: readonly string[]) => string,
): string {
  const t = dict.keysAdvanced;
  switch (def.type) {
    case "dks":
      return dksSentence(def, dict, lang, list);
    case "mod_tap":
      return t.mt.sentence({
        tap: usageName(def.tap, dict),
        hold: usageName(def.hold, dict),
        ms: t.mt.ms({ n: def.hold_ms }),
      });
    case "toggle":
      return t.toggle.sentence({ key: usageName(def.key, dict) });
  }
}

// ---------- the draft ----------

/** One unwritten advanced-key change. */
export type DraftItem =
  /** A new advanced key bound to a key (the library chooses the slot). */
  | { id: string; kind: "create"; layer: LayerView; key: number; def: AdvancedDefView }
  /** A slot's definition changed (its keys stay). */
  | { id: string; kind: "edit"; slot: number; def: AdvancedDefView }
  /** A slot no key uses, assigned to a key. */
  | { id: string; kind: "assign"; slot: number; layer: LayerView; key: number }
  /** A slot deleted: every key that uses it back to Default, and the slot emptied, in one write. */
  | { id: string; kind: "delete"; slot: number };

/** The tab's unwritten changes and the critical built-ins the user confirmed for them. */
export interface AdvancedDraft {
  items: readonly DraftItem[];
  acks: readonly BuiltinAckIntent[];
}

/** No changes. */
export const EMPTY_DRAFT: AdvancedDraft = { items: [], acks: [] };

/** A draft item without its id (the store gives one). */
export type NewDraftItem =
  | Omit<Extract<DraftItem, { kind: "create" }>, "id">
  | Omit<Extract<DraftItem, { kind: "edit" }>, "id">
  | Omit<Extract<DraftItem, { kind: "assign" }>, "id">
  | Omit<Extract<DraftItem, { kind: "delete" }>, "id">;

function targetsKey(item: DraftItem, layer: LayerView, key: number): boolean {
  return (item.kind === "create" || item.kind === "assign") && item.layer === layer && item.key === key;
}

function targetsSlot(item: DraftItem, slot: number): boolean {
  return item.kind !== "create" && item.slot === slot;
}

/**
 * Adds a change to the draft, replacing the changes it supersedes: one
 * change per slot (a delete replaces an edit or an assignment of the slot),
 * one new key per key and layer. `replaces` names an item being re-edited.
 */
export function addItem(draft: AdvancedDraft, item: DraftItem, replaces: string | null = null): AdvancedDraft {
  const superseded = (existing: DraftItem): boolean => {
    if (existing.id === replaces) return true;
    if ((item.kind === "create" || item.kind === "assign") && targetsKey(existing, item.layer, item.key)) return true;
    return item.kind !== "create" && targetsSlot(existing, item.slot);
  };
  // A re-edited item keeps its place; anything else goes last.
  const next: DraftItem[] = [];
  let at = -1;
  for (const existing of draft.items) {
    if (existing.id === replaces) at = next.length;
    if (!superseded(existing)) next.push(existing);
  }
  if (at >= 0) next.splice(at, 0, item);
  else next.push(item);
  return { ...draft, items: next };
}

/** Removes one change from the draft. */
export function removeItem(draft: AdvancedDraft, id: string): AdvancedDraft {
  return { ...draft, items: draft.items.filter((item) => item.id !== id) };
}

/** The library's change of a draft item. */
export function changeOf(item: DraftItem): KeymapChangeIntent {
  switch (item.kind) {
    case "create":
      return change.bindAdvanced(item.layer, item.key, item.def, null);
    case "edit":
      return change.setAdvanced(item.slot, item.def);
    case "assign":
      return change.assign(item.layer, item.key, { kind: "advanced", slot: item.slot });
    case "delete":
      return change.deleteAdvanced(item.slot);
  }
}

/**
 * The library's changes of the draft: the deletes first (a delete empties
 * its slot and sets its keys back to Default, so a new advanced key drafted
 * on one of those keys must come after it, or the library would put the new
 * key in the slot being deleted), then the others in the order they were made.
 */
export function changesOf(draft: AdvancedDraft): KeymapChangeIntent[] {
  const deletes = draft.items.filter((item) => item.kind === "delete");
  const others = draft.items.filter((item) => item.kind !== "delete");
  return [...deletes, ...others].map(changeOf);
}

/** The confirmed critical built-ins the draft's changes still need (passed to the write flow). */
export function draftAcks(draft: AdvancedDraft, state: KeysState | null): BuiltinAckIntent[] {
  const needed = neededAcks(changesOf(draft), state).filter((need) => need.critical);
  return draft.acks.filter((ack) => ack.critical && needed.some((need) => need.key === ack.key));
}

/** Adds (or replaces) the confirmation of a critical built-in. */
export function withAck(draft: AdvancedDraft, ack: BuiltinAckIntent | null): AdvancedDraft {
  if (ack === null) return draft;
  return { ...draft, acks: [...draft.acks.filter((existing) => existing.key !== ack.key), ack] };
}

// ---------- the slots as the draft leaves them ----------

/** A used slot with the draft's change of it. */
export interface SlotCardModel {
  slot: AdvancedSlotStateView;
  /** The drafted definition (edit), the drafted assignment, or the delete. */
  change: Extract<DraftItem, { kind: "edit" | "assign" | "delete" }> | null;
}

/** The cards of the used slots (slot order). */
export function slotCards(state: KeysState, draft: AdvancedDraft): SlotCardModel[] {
  return state.advanced
    .filter((slot) => slot.content.state !== "empty")
    .map((slot) => {
      const found = draft.items.find((item) => targetsSlot(item, slot.slot));
      return { slot, change: found !== undefined && found.kind !== "create" ? found : null };
    });
}

/** The new advanced keys of the draft. */
export function createdItems(draft: AdvancedDraft): Extract<DraftItem, { kind: "create" }>[] {
  return draft.items.filter((item): item is Extract<DraftItem, { kind: "create" }> => item.kind === "create");
}

/**
 * The slot the library would choose for a new advanced key on a key: the
 * slot only that key uses, else the lowest free one (`null` when none is
 * free). Shown only in the technical details.
 */
export function slotFor(state: KeysState, layer: LayerView, key: number): number | null {
  const view = state.keys.find((item) => item.key.index === key);
  const entry = layer === "base" ? view?.base : view?.fn_layer;
  if (entry?.origin.origin === "advanced_slot") {
    const slot = entry.origin.slot;
    const bindings = state.advanced.find((item) => item.slot === slot)?.bindings ?? [];
    if (bindings.every((binding) => binding.layer === layer && binding.key.index === key)) return slot;
  }
  return state.advanced.find((item) => item.content.state === "empty" && item.bindings.length === 0)?.slot ?? null;
}

/** Whether a new advanced key has a slot to go to. */
export function hasFreeSlot(state: KeysState): boolean {
  return state.advanced.some((item) => item.content.state === "empty");
}
