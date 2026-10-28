// The action catalogue (generated from the library: `ACTION_CATALOG`) as the
// UI uses it: stable ids, localized names, search in English and Turkish,
// the evidence of an action on a layer, and shortcut building. Nothing here
// keeps its own list of verified or unverified actions: evidence always
// comes from the catalogue (the library's `Evidence`).

import { ACTION_CATALOG } from "../../generated/actionCatalog";
import { DICTIONARIES, keyName, type Dict } from "../../i18n";
import type {
  ActionCatalog,
  ActionValue,
  CatalogEntry,
  CatalogModifier,
  EvidenceView,
  LayerView,
  UnverifiedNoteView,
} from "../../ipc/bindings";

export { ACTION_CATALOG };

/** A byte as two lowercase hex digits ("04"). */
export function hex2(value: number): string {
  return (value & 0xff).toString(16).padStart(2, "0");
}

/**
 * The stable id of an action (`default`, `key:04`, `media:12`, `mouse:back`,
 * `combo:e0+06`, `macro:3`, `advanced:12`), as the library forms it.
 */
export function actionId(value: ActionValue): string {
  switch (value.kind) {
    case "default":
      return "default";
    case "key":
      return `key:${hex2(value.usage)}`;
    case "media":
      return `media:${hex2(value.index)}`;
    case "mouse":
      return `mouse:${value.action}`;
    case "combo":
      return `combo:${[...value.modifiers, value.key].map(hex2).join("+")}`;
    case "macro":
      return `macro:${value.slot}`;
    case "advanced":
      return `advanced:${value.slot}`;
  }
}

/** Whether two actions are the same. */
export function sameAction(a: ActionValue | null | undefined, b: ActionValue | null | undefined): boolean {
  if (a === null || a === undefined || b === null || b === undefined) return a === b;
  return actionId(a) === actionId(b);
}

const entriesById = new Map<string, CatalogEntry>(ACTION_CATALOG.entries.map((entry) => [entry.id, entry]));

/** The catalogue entry of an id (only fixed kinds: default, keys, media, mouse). */
export function catalogEntry(id: string, catalog: ActionCatalog = ACTION_CATALOG): CatalogEntry | undefined {
  return catalog === ACTION_CATALOG ? entriesById.get(id) : catalog.entries.find((entry) => entry.id === id);
}

/** The catalogue entry of a HID usage (`key:<usage>`). */
export function usageEntry(usage: number, catalog: ActionCatalog = ACTION_CATALOG): CatalogEntry | undefined {
  return catalogEntry(`key:${hex2(usage)}`, catalog);
}

/** The modifier of a usage (E0..E7), if it is one. */
export function modifierOf(usage: number, catalog: ActionCatalog = ACTION_CATALOG): CatalogModifier | undefined {
  return catalog.modifiers.find((modifier) => modifier.usage === usage);
}

/** A display name in `dict`'s language: the dictionary's name for the id, else the key's name, else the library's. */
export function entryName(entry: Pick<CatalogEntry, "id" | "name">, dict: Dict): string {
  const names: Readonly<Record<string, string>> = dict.keys.actionNames;
  const named = names[entry.id];
  if (named !== undefined) return named;
  if (entry.name.startsWith("KP ")) return dict.keys.action.keypad({ key: entry.name.slice(3) });
  return keyName(dict, entry.name);
}

/** The name of a HID usage ("A", "Left Ctrl"); `0x..` for one the catalogue doesn't know. */
export function usageName(usage: number, dict: Dict, catalog: ActionCatalog = ACTION_CATALOG): string {
  const entry = usageEntry(usage, catalog);
  return entry === undefined ? `0x${hex2(usage)}` : entryName(entry, dict);
}

/** A shortcut's text ("Left Ctrl + C"). */
export function comboName(modifiers: readonly number[], key: number, dict: Dict): string {
  return [...modifiers, key].map((usage) => usageName(usage, dict)).join(" + ");
}

/**
 * Names of an action outside slots (macros and advanced keys are named by
 * the caller, which knows the slots): `null` for a slot reference.
 */
export function fixedActionName(value: ActionValue, dict: Dict): string | null {
  switch (value.kind) {
    case "default":
      return dict.keys.action.default;
    case "combo":
      return comboName(value.modifiers, value.key, dict);
    case "macro":
    case "advanced":
      return null;
    case "key":
    case "media":
    case "mouse": {
      const entry = catalogEntry(actionId(value));
      return entry === undefined ? actionId(value) : entryName(entry, dict);
    }
  }
}

/** The evidence of an entry on a layer; `null` when the entry isn't offered on that layer. */
export function layerEvidence(entry: CatalogEntry, layer: LayerView): EvidenceView | null {
  return entry.layers.find((item) => item.layer === layer)?.evidence ?? null;
}

/** The evidence of binding a macro or an advanced key on a layer (the catalogue's binding rules). */
export function bindingEvidence(
  kind: "macro" | "advanced",
  layer: LayerView,
  catalog: ActionCatalog = ACTION_CATALOG,
): EvidenceView | null {
  const rule = catalog.bindings.find((item) => item.kind === kind);
  return rule?.layers.find((item) => item.layer === layer)?.evidence ?? null;
}

/** Joins evidence: unverified when any part is, with every note once. */
export function combineEvidence(...parts: readonly (EvidenceView | null | undefined)[]): EvidenceView {
  const notes: UnverifiedNoteView[] = [];
  for (const part of parts) {
    if (part?.status !== "unverified") continue;
    for (const note of part.notes) {
      if (!notes.some((n) => n.what === note.what && n.question === note.question)) notes.push(note);
    }
  }
  return notes.length === 0 ? { status: "confirmed" } : { status: "unverified", notes };
}

/**
 * The evidence of a shortcut: the key's layer evidence plus every chosen
 * modifier's `combo_evidence` (the library's rule).
 */
export function comboEvidence(
  modifiers: readonly number[],
  key: number,
  layer: LayerView,
  catalog: ActionCatalog = ACTION_CATALOG,
): EvidenceView {
  const entry = usageEntry(key, catalog);
  return combineEvidence(
    entry === undefined ? null : layerEvidence(entry, layer),
    ...modifiers.map((usage) => modifierOf(usage, catalog)?.combo_evidence),
  );
}

/** Whether a shortcut is valid: 1-2 distinct catalogue modifiers and one `combo_key` entry. */
export function isValidCombo(
  modifiers: readonly number[],
  key: number | null,
  catalog: ActionCatalog = ACTION_CATALOG,
): boolean {
  const { min_modifiers: min, max_modifiers: max } = catalog.combo;
  if (key === null || modifiers.length < min || modifiers.length > max) return false;
  if (new Set(modifiers).size !== modifiers.length) return false;
  if (!modifiers.every((usage) => modifierOf(usage, catalog) !== undefined)) return false;
  return usageEntry(key, catalog)?.combo_key === true;
}

/** Folds text for search: lower case, no diacritics, Turkish dotted/dotless i alike. */
export function fold(text: string): string {
  return text
    .toLocaleLowerCase("tr")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/ı/g, "i")
    .replace(/\s+/g, " ")
    .trim();
}

/** Every text an entry is found by: names in English and Turkish, the library name, aliases, hex. */
export function searchTexts(entry: CatalogEntry): string[] {
  const texts = new Set<string>([entry.name, entry.id, entry.hex, ...entry.aliases]);
  for (const dict of Object.values(DICTIONARIES)) {
    texts.add(entryName(entry, dict));
    texts.add(dict.keys.groups[entry.group]);
  }
  return [...texts];
}

const searchIndex = new Map<string, string[]>();

function foldedTexts(entry: CatalogEntry): string[] {
  let texts = searchIndex.get(entry.id);
  if (texts === undefined) {
    texts = searchTexts(entry).map(fold);
    searchIndex.set(entry.id, texts);
  }
  return texts;
}

/**
 * How well an entry matches a query (higher is better; 0 = no match). Every
 * word of the query has to match the start of a word or be contained;
 * exact names and hex codes rank first.
 */
export function matchScore(entry: CatalogEntry, query: string): number {
  const q = fold(query).replace(/^0x/, "");
  if (q === "") return 1;
  const texts = foldedTexts(entry);
  const words = q.split(" ");
  let score = 0;
  for (const word of words) {
    let best = 0;
    for (const text of texts) {
      const bare = text.replace(/^0x/, "");
      if (text === word || bare === word) best = Math.max(best, 100);
      else if (text.startsWith(word) || bare.startsWith(word)) best = Math.max(best, 60);
      else if (text.split(/[\s/()+-]/).some((part) => part.startsWith(word))) best = Math.max(best, 40);
      else if (text.includes(word)) best = Math.max(best, 10);
    }
    if (best === 0) return 0;
    score += best;
  }
  return score;
}

/** The entries matching `query` on `layer` (offered there), best first, catalogue order within a score. */
export function searchCatalog(
  query: string,
  layer: LayerView,
  catalog: ActionCatalog = ACTION_CATALOG,
): CatalogEntry[] {
  return catalog.entries
    .map((entry, order) => ({ entry, order, score: layerEvidence(entry, layer) === null ? 0 : matchScore(entry, query) }))
    .filter((item) => item.score > 0)
    .sort((a, b) => b.score - a.score || a.order - b.order)
    .map((item) => item.entry);
}

const MODIFIER_WORDS: readonly { words: readonly string[]; left: number; right: number }[] = [
  { words: ["ctrl", "control", "kontrol"], left: 0xe0, right: 0xe4 },
  { words: ["shift"], left: 0xe1, right: 0xe5 },
  { words: ["alt"], left: 0xe2, right: 0xe6 },
  { words: ["win", "windows", "meta", "cmd", "super"], left: 0xe3, right: 0xe7 },
];

const RIGHT_WORDS = ["right", "r", "sag"];
const LEFT_WORDS = ["left", "l", "sol"];

/**
 * A shortcut typed into the search ("ctrl c", "Ctrl + Shift + S", "sağ alt
 * 4"): 1-2 modifiers and one key, or `null` when the text isn't one.
 */
export function parseComboQuery(
  query: string,
  catalog: ActionCatalog = ACTION_CATALOG,
): { modifiers: number[]; key: number } | null {
  const words = fold(query)
    .replace(/[+]/g, " ")
    .replace(/-/g, " ")
    .split(" ")
    .filter((word) => word !== "");
  const modifiers: number[] = [];
  let side: "left" | "right" | null = null;
  let key: number | null = null;
  const isModifier = (word: string | undefined) =>
    word !== undefined && MODIFIER_WORDS.some((item) => item.words.includes(word));
  for (const [i, word] of words.entries()) {
    // A side word only before a modifier ("right ctrl"); otherwise "r" and "l" are keys ("ctrl r").
    if (RIGHT_WORDS.includes(word) && isModifier(words[i + 1])) {
      side = "right";
      continue;
    }
    if (LEFT_WORDS.includes(word) && isModifier(words[i + 1])) {
      side = "left";
      continue;
    }
    const modifier = MODIFIER_WORDS.find((item) => item.words.includes(word));
    if (modifier !== undefined) {
      modifiers.push(side === "right" ? modifier.right : modifier.left);
      side = null;
      continue;
    }
    if (key !== null) return null;
    const entry = catalog.entries.find(
      (item) =>
        item.combo_key &&
        (fold(item.name) === word || item.aliases.some((alias) => fold(alias) === word || fold(alias) === `key${word}`)),
    );
    if (entry === undefined || entry.value.kind !== "key") return null;
    key = entry.value.usage;
  }
  if (key === null || !isValidCombo(modifiers, key, catalog)) return null;
  return { modifiers, key };
}
