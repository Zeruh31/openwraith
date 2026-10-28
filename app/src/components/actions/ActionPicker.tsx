// The action picker: what a key does. A search field (combobox) drives a
// listbox through `aria-activedescendant`, so the focus stays in the field
// while Up/Down move through the options and Enter picks. With an empty
// search the options come in tabs (Keys · Media · Mouse · Shortcuts ·
// Macros · Advanced · Default); a search looks through every tab: English
// and Turkish names, the library names, `KeyboardEvent.code` aliases and hex
// codes ("0x04"), and a typed shortcut ("ctrl c") is offered first.
//
// Layer rules come from the catalogue: an action not offered on the layer is
// not listed, and every badge is the catalogue's evidence for that layer (or
// the binding rule's, for macros and advanced keys). Picking only reports
// the value; nothing is written from here.

import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

import { useDict, type Dict } from "../../i18n";
import type {
  ActionCatalog,
  ActionValue,
  CatalogEntry,
  CatalogGroup,
  CatalogTab,
  EvidenceView,
  LayerView,
} from "../../ipc/bindings";
import { EvidenceBadge } from "../Badge";
import { Button } from "../Button";
import { Dialog } from "../Dialog";
import { Icon } from "../Icon";
import {
  ACTION_CATALOG,
  actionId,
  bindingEvidence,
  combineEvidence,
  comboEvidence,
  comboName,
  entryName,
  isValidCombo,
  layerEvidence,
  parseComboQuery,
  sameAction,
  searchCatalog,
} from "./catalog";

/** A macro or advanced-key slot offered by the picker (the caller knows the slots). */
export interface ActionSlotOption {
  /** The library's slot (0-based). */
  slot: number;
  /** The name shown ("Macro 3 · Type A"). */
  label: string;
  /** A second line ("4 steps · plays once · used by Q"). */
  description?: string | undefined;
  /** The slot content's evidence; `null` for content kept as read. */
  evidence: EvidenceView | null;
  /** Why it can't be chosen (e.g. another key uses it); `null`/absent when it can. */
  disabledReason?: string | null | undefined;
}

/** Every tab, in display order. */
export const PICKER_TABS: readonly CatalogTab[] = ["keys", "media", "mouse", "shortcuts", "macros", "advanced", "default"];

/** Props of {@link ActionPicker}. */
export interface ActionPickerProps {
  /** The layer the action goes on (decides what is offered and the badges). */
  layer: LayerView;
  /** The key's current action (marked "Current"). */
  value: ActionValue | null;
  onPick: (value: ActionValue) => void;
  /** The keyboard's macros to offer (the Macros tab is shown only with this). */
  macros?: readonly ActionSlotOption[] | undefined;
  /** The keyboard's advanced keys to offer (the Advanced tab is shown only with this). */
  advanced?: readonly ActionSlotOption[] | undefined;
  /** The tabs to show (default: all that have content). */
  tabs?: readonly CatalogTab[] | undefined;
  /** The tab shown first (default: the current action's). */
  initialTab?: CatalogTab | undefined;
  catalog?: ActionCatalog | undefined;
  /** Focus the search field on mount (default true). */
  autoFocus?: boolean | undefined;
}

/** One option of the list. */
interface PickerOption {
  key: string;
  value: ActionValue;
  name: string;
  detail: string | null;
  evidence: EvidenceView | null;
  /** No evidence means kept as read (slot content OpenWraith doesn't decode). */
  kept: boolean;
  disabledReason: string | null;
  current: boolean;
  group: string | null;
}

function tabOfValue(value: ActionValue | null): CatalogTab {
  switch (value?.kind) {
    case undefined:
    case "key":
      return "keys";
    case "media":
      return "media";
    case "mouse":
      return "mouse";
    case "combo":
      return "shortcuts";
    case "macro":
      return "macros";
    case "advanced":
      return "advanced";
    case "default":
      return "default";
  }
}

function groupName(group: CatalogGroup, dict: Dict): string {
  return dict.keys.groups[group];
}

function entryOption(entry: CatalogEntry, layer: LayerView, value: ActionValue | null, dict: Dict, withGroup: boolean): PickerOption {
  return {
    key: entry.id,
    value: entry.value,
    name: entryName(entry, dict),
    detail: withGroup ? `${dict.keys.picker.tabs[entry.tab]} · ${groupName(entry.group, dict)}` : null,
    evidence: layerEvidence(entry, layer),
    kept: false,
    disabledReason: null,
    current: sameAction(entry.value, value),
    group: groupName(entry.group, dict),
  };
}

function slotOptions(
  kind: "macro" | "advanced",
  slots: readonly ActionSlotOption[],
  layer: LayerView,
  value: ActionValue | null,
  catalog: ActionCatalog,
): PickerOption[] {
  const rule = bindingEvidence(kind, layer, catalog);
  return slots.map((slot) => {
    const action: ActionValue = kind === "macro" ? { kind: "macro", slot: slot.slot } : { kind: "advanced", slot: slot.slot };
    return {
      key: actionId(action),
      value: action,
      name: slot.label,
      detail: slot.description ?? null,
      evidence: slot.evidence === null ? null : combineEvidence(rule, slot.evidence),
      kept: slot.evidence === null,
      disabledReason: slot.disabledReason ?? null,
      current: sameAction(action, value),
      group: null,
    };
  });
}

/** The action picker (a search combobox, tabs and a listbox). */
export function ActionPicker({
  layer,
  value,
  onPick,
  macros,
  advanced,
  tabs,
  initialTab,
  catalog = ACTION_CATALOG,
  autoFocus = true,
}: ActionPickerProps) {
  const dict = useDict();
  const t = dict.keys.picker;
  const base = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const shown = useMemo(
    () =>
      (tabs ?? PICKER_TABS).filter(
        (tab) => (tab !== "macros" || macros !== undefined) && (tab !== "advanced" || advanced !== undefined),
      ),
    [tabs, macros, advanced],
  );
  const [tab, setTab] = useState<CatalogTab>(() => {
    const wanted = initialTab ?? tabOfValue(value);
    return shown.includes(wanted) ? wanted : (shown[0] ?? "keys");
  });
  const [query, setQuery] = useState("");
  const searching = query.trim() !== "";

  useEffect(() => {
    if (autoFocus) inputRef.current?.focus();
  }, [autoFocus]);

  const options = useMemo((): PickerOption[] => {
    if (searching) {
      const found: PickerOption[] = [];
      const combo = shown.includes("shortcuts") ? parseComboQuery(query, catalog) : null;
      if (combo !== null) {
        const action: ActionValue = { kind: "combo", modifiers: combo.modifiers, key: combo.key };
        found.push({
          key: actionId(action),
          value: action,
          name: comboName(combo.modifiers, combo.key, dict),
          detail: t.tabs.shortcuts,
          evidence: comboEvidence(combo.modifiers, combo.key, layer, catalog),
          kept: false,
          disabledReason: null,
          current: sameAction(action, value),
          group: null,
        });
      }
      for (const entry of searchCatalog(query, layer, catalog)) {
        if (shown.includes(entry.tab)) found.push(entryOption(entry, layer, value, dict, true));
      }
      const words = query.trim().toLocaleLowerCase();
      const slotMatches = (option: PickerOption) =>
        option.name.toLocaleLowerCase().includes(words) || (option.detail?.toLocaleLowerCase().includes(words) ?? false);
      if (macros !== undefined && shown.includes("macros")) {
        found.push(...slotOptions("macro", macros, layer, value, catalog).filter(slotMatches));
      }
      if (advanced !== undefined && shown.includes("advanced")) {
        found.push(...slotOptions("advanced", advanced, layer, value, catalog).filter(slotMatches));
      }
      return found;
    }
    switch (tab) {
      case "macros":
        return slotOptions("macro", macros ?? [], layer, value, catalog);
      case "advanced":
        return slotOptions("advanced", advanced ?? [], layer, value, catalog);
      case "shortcuts":
        return [];
      default:
        return catalog.entries
          .filter((entry) => entry.tab === tab && layerEvidence(entry, layer) !== null)
          .map((entry) => entryOption(entry, layer, value, dict, false));
    }
  }, [searching, query, tab, shown, catalog, layer, value, dict, macros, advanced, t]);

  // A new list starts at the current action (or the first option); the
  // active option is remembered together with the list it belongs to.
  const [activeIn, setActiveIn] = useState<{ list: readonly PickerOption[]; index: number } | null>(null);
  const start = Math.max(0, options.findIndex((option) => option.current));
  const active = activeIn !== null && activeIn.list === options ? activeIn.index : start;
  const setActive = (next: number | ((index: number) => number)) => {
    const index = typeof next === "function" ? next(active) : next;
    setActiveIn({ list: options, index: Math.max(0, Math.min(options.length - 1, index)) });
  };

  const optionId = (index: number) => `${base}-o-${index}`;
  const activeOption = options[active];

  useEffect(() => {
    const element = document.getElementById(optionId(active));
    if (element !== null && typeof element.scrollIntoView === "function") element.scrollIntoView({ block: "nearest" });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the id depends only on the index
  }, [active, options]);

  const pick = (option: PickerOption | undefined) => {
    if (option === undefined || option.disabledReason !== null) return;
    onPick(option.value);
  };

  const onInputKey = (event: KeyboardEvent<HTMLInputElement>) => {
    const last = options.length - 1;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setActive((index) => Math.min(last, index + 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive((index) => Math.max(0, index - 1));
        break;
      case "PageDown":
        event.preventDefault();
        setActive((index) => Math.min(last, index + 8));
        break;
      case "PageUp":
        event.preventDefault();
        setActive((index) => Math.max(0, index - 8));
        break;
      case "Enter":
        event.preventDefault();
        pick(activeOption);
        break;
      case "Escape":
        if (query !== "") {
          event.preventDefault();
          event.stopPropagation();
          setQuery("");
        }
        break;
      default:
        break;
    }
  };

  const onTabKey = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    let next = -1;
    if (event.key === "ArrowRight") next = (index + 1) % shown.length;
    if (event.key === "ArrowLeft") next = (index - 1 + shown.length) % shown.length;
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = shown.length - 1;
    const target = shown[next];
    if (target !== undefined) {
      event.preventDefault();
      tabRefs.current[next]?.focus();
      setTab(target);
    }
  };

  const renderOption = (option: PickerOption, index: number) => (
    <div
      key={`${option.key}-${index}`}
      id={optionId(index)}
      role="option"
      aria-selected={index === active}
      aria-disabled={option.disabledReason !== null || undefined}
      className={[
        "ow-picker__option",
        index === active ? "ow-picker__option--active" : "",
        option.current ? "ow-picker__option--current" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      title={option.disabledReason ?? undefined}
      onPointerMove={() => {
        if (index !== active) setActive(index);
      }}
      onClick={() => {
        setActive(index);
        pick(option);
      }}
    >
      <span className="ow-picker__option-main">
        <span className="ow-picker__option-name">{option.name}</span>
        {option.current ? <span className="ow-picker__current">{t.current}</span> : null}
        <EvidenceBadge evidence={option.evidence} keptWhenNone={option.kept} size="sm" />
      </span>
      {option.detail === null && option.disabledReason === null ? null : (
        <span className="ow-picker__option-detail ow-small ow-muted">
          {[option.detail, option.disabledReason].filter((part) => part !== null).join(" · ")}
        </span>
      )}
    </div>
  );

  let list: ReactNode;
  if (options.length === 0) {
    list = null;
  } else if (!searching && (tab === "keys" || tab === "media" || tab === "mouse")) {
    // Grouped by the catalogue's groups (letters, digits, playback, …).
    const groups: { name: string; items: { option: PickerOption; index: number }[] }[] = [];
    options.forEach((option, index) => {
      const name = option.group ?? "";
      const last = groups[groups.length - 1];
      if (last !== undefined && last.name === name) last.items.push({ option, index });
      else groups.push({ name, items: [{ option, index }] });
    });
    list = groups.map((group, g) => (
      <div key={`${g}-${group.name}`} role="group" aria-labelledby={`${base}-g-${g}`} className="ow-picker__group">
        <div id={`${base}-g-${g}`} role="presentation" className="ow-picker__group-name">
          {group.name}
        </div>
        {group.items.map(({ option, index }) => renderOption(option, index))}
      </div>
    ));
  } else {
    list = options.map(renderOption);
  }

  const emptyText =
    searching
      ? t.noResults
      : tab === "macros"
        ? t.macros.empty
        : tab === "advanced"
          ? t.advanced.empty
          : null;

  return (
    <div className="ow-picker">
      <div className="ow-picker__search">
        <Icon name="search" size={16} className="ow-picker__search-icon" />
        <input
          ref={inputRef}
          type="text"
          className="ow-input ow-picker__input"
          role="combobox"
          aria-label={t.search}
          aria-describedby={`${base}-hint`}
          aria-expanded="true"
          aria-controls={`${base}-list`}
          aria-autocomplete="list"
          aria-activedescendant={activeOption === undefined ? undefined : optionId(active)}
          autoComplete="off"
          spellCheck={false}
          value={query}
          onChange={(event) => {
            setQuery(event.currentTarget.value);
          }}
          onKeyDown={onInputKey}
        />
      </div>
      <p id={`${base}-hint`} className="ow-small ow-muted">
        {shown.includes("shortcuts") ? t.searchHint : t.searchHintKeys}
      </p>
      {searching ? (
        <p className="ow-small ow-muted" role="status">
          {t.results({ n: options.length })}
        </p>
      ) : (
        <div className="ow-picker__tabs" role="tablist" aria-label={t.tabsLabel}>
          {shown.map((item, index) => (
            <button
              key={item}
              ref={(element) => {
                tabRefs.current[index] = element;
              }}
              id={`${base}-t-${item}`}
              type="button"
              role="tab"
              aria-selected={item === tab}
              aria-controls={`${base}-panel`}
              tabIndex={item === tab ? 0 : -1}
              className="ow-picker__tab"
              onClick={() => {
                setTab(item);
              }}
              onKeyDown={(event) => {
                onTabKey(event, index);
              }}
            >
              {t.tabs[item]}
            </button>
          ))}
        </div>
      )}
      <div
        id={`${base}-panel`}
        className="ow-picker__panel"
        role={searching ? undefined : "tabpanel"}
        aria-labelledby={searching ? undefined : `${base}-t-${tab}`}
      >
        {!searching && tab === "shortcuts" ? (
          <ShortcutBuilder layer={layer} value={value} catalog={catalog} onPick={onPick} />
        ) : null}
        {!searching && tab === "default" ? <p className="ow-small ow-muted">{t.defaultHint}</p> : null}
        <div id={`${base}-list`} role="listbox" aria-label={t.options} className="ow-picker__list">
          {list}
        </div>
        {options.length === 0 && emptyText !== null ? <p className="ow-muted ow-picker__empty">{emptyText}</p> : null}
      </div>
    </div>
  );
}

/** Props of {@link ShortcutBuilder}. */
interface ShortcutBuilderProps {
  layer: LayerView;
  value: ActionValue | null;
  catalog: ActionCatalog;
  onPick: (value: ActionValue) => void;
}

/** One or two modifier toggles plus one key. */
function ShortcutBuilder({ layer, value, catalog, onPick }: ShortcutBuilderProps) {
  const dict = useDict();
  const t = dict.keys.picker.shortcut;
  const id = useId();
  const [modifiers, setModifiers] = useState<number[]>(() => (value?.kind === "combo" ? [...value.modifiers] : []));
  const [key, setKey] = useState<number | null>(() => (value?.kind === "combo" ? value.key : null));
  const max = catalog.combo.max_modifiers;
  const keys = useMemo(
    () =>
      catalog.entries.filter(
        (entry) => entry.combo_key && entry.value.kind === "key" && layerEvidence(entry, layer) !== null,
      ),
    [catalog, layer],
  );
  const valid = isValidCombo(modifiers, key, catalog);
  const evidence = key === null ? null : comboEvidence(modifiers, key, layer, catalog);
  const reason = modifiers.length === 0 ? t.needModifier : key === null ? t.needKey : null;

  const toggle = (usage: number) => {
    setModifiers((current) =>
      current.includes(usage) ? current.filter((u) => u !== usage) : current.length >= max ? current : [...current, usage],
    );
  };

  // Keys grouped as in the catalogue.
  const groups: { group: CatalogGroup; entries: CatalogEntry[] }[] = [];
  for (const entry of keys) {
    const last = groups[groups.length - 1];
    if (last?.group === entry.group) last.entries.push(entry);
    else groups.push({ group: entry.group, entries: [entry] });
  }

  return (
    <div className="ow-shortcut">
      <fieldset className="ow-shortcut__modifiers">
        <legend className="ow-small ow-strong">{t.modifiers}</legend>
        <div className="ow-shortcut__toggles">
          {catalog.modifiers.map((modifier) => {
            const on = modifiers.includes(modifier.usage);
            const full = !on && modifiers.length >= max;
            const name = entryName({ id: `key:${modifier.usage.toString(16)}`, name: modifier.name }, dict);
            return (
              <button
                key={modifier.usage}
                type="button"
                className="ow-shortcut__toggle"
                aria-pressed={on}
                aria-disabled={full || undefined}
                title={full ? t.max : undefined}
                onClick={() => {
                  if (!full) toggle(modifier.usage);
                }}
              >
                <span>{name}</span>
                <EvidenceBadge evidence={modifier.combo_evidence} size="sm" />
              </button>
            );
          })}
        </div>
      </fieldset>
      <div className="ow-shortcut__key">
        <label htmlFor={`${id}-key`} className="ow-small ow-strong">
          {t.key}
        </label>
        <select
          id={`${id}-key`}
          className="ow-input"
          value={key === null ? "" : String(key)}
          aria-describedby={`${id}-key-hint`}
          onChange={(event) => {
            const next = event.currentTarget.value;
            setKey(next === "" ? null : Number(next));
          }}
        >
          <option value="">{t.needKey}</option>
          {groups.map((group, g) => (
            <optgroup key={`${g}-${group.group}`} label={dict.keys.groups[group.group]}>
              {group.entries.map((entry) =>
                entry.value.kind === "key" ? (
                  <option key={entry.id} value={entry.value.usage}>
                    {entryName(entry, dict)}
                  </option>
                ) : null,
              )}
            </optgroup>
          ))}
        </select>
        <p id={`${id}-key-hint`} className="ow-small ow-muted">
          {t.keyPlaceholderHint}
        </p>
      </div>
      <div className="ow-shortcut__use">
        <Button
          variant="primary"
          size="sm"
          disabledReason={valid ? null : reason}
          onClick={() => {
            if (valid && key !== null) onPick({ kind: "combo", modifiers, key });
          }}
        >
          {key === null || modifiers.length === 0 ? t.use({ combo: "…" }) : t.use({ combo: comboName(modifiers, key, dict) })}
        </Button>
        {evidence === null ? null : <EvidenceBadge evidence={evidence} />}
      </div>
    </div>
  );
}

/** Props of {@link ActionPickerDialog}. */
export interface ActionPickerDialogProps extends Omit<ActionPickerProps, "autoFocus"> {
  open: boolean;
  /** The dialog's title ("Change Fn + Q"). */
  title: ReactNode;
  onClose: () => void;
  /** Shown above the picker (e.g. a built-in warning). */
  intro?: ReactNode | undefined;
}

/** The picker in a dialog (at most 760 px wide); picking reports the value, the caller closes it. */
export function ActionPickerDialog({ open, title, onClose, intro, ...picker }: ActionPickerDialogProps) {
  const dict = useDict();
  return (
    <Dialog
      open={open}
      title={title}
      onClose={onClose}
      size="lg"
      className="ow-picker-dialog"
      actions={<Button onClick={onClose}>{dict.common.actions.cancel}</Button>}
    >
      {open ? (
        <>
          {intro}
          <ActionPicker {...picker} />
        </>
      ) : null}
    </Dialog>
  );
}
