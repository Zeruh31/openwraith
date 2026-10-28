// "On the keyboard (profile N)": the keyboard's 10 macro slots, each with its
// name (from the library when it knows the steps), what it holds, the keys
// that run it, and what the draft does to it; macros no key runs are marked
// (⚠, with Assign… and Delete), as are keys left on an empty slot. New
// macros of the draft follow the slots. Nothing here writes: every button
// changes the draft or opens the editor.
//
// Macro 1 (slot 0) is special: writing it makes the keyboard empty Macros
// 2-10 (OpenWraith writes them back right after it). An empty Macro 1 that
// no key runs offers no Create… (New macro… puts a macro in Macros 2-10
// first, unless its key already runs a slot on its own: the library reuses
// that one, and the new row and the editor say so); a Macro 1 that holds
// something or that a key runs says so on its row.

import { Badge, EvidenceBadge } from "../../../components/Badge";
import { Button } from "../../../components/Button";
import { Icon } from "../../../components/Icon";
import { Card } from "../../../components/Layout";
import { useDict, type Dict } from "../../../i18n";
import type { KeysState, LibraryMacroView, MacroProgramView, MacroSlotStateView } from "../../../ipc/bindings";
import { bindingsText, keyAt, keyOnLayer, macroName, programText, type LabelContext } from "../shared";
import {
  canBind,
  canRemove,
  editOf,
  libraryMatch,
  ownMacroSlot,
  slotProgram,
  type Binding,
  type MacrosDraft,
  type NewMacro,
  type Target,
} from "./model";

/** A binding of the draft in words ("Fn + R"). */
export function bindText(bind: Binding, state: KeysState, dict: Dict): string {
  const key = keyAt(state, bind.key);
  return keyOnLayer(key?.key.name ?? `#${bind.key}`, bind.layer, dict);
}

/** The name of a new macro ("New macro", "New macro 2"). */
export function newName(draft: MacrosDraft, id: number, dict: Dict): string {
  const index = draft.added.findIndex((item) => item.id === id);
  return dict.keysMacros.slots.newRow({ n: index < 0 ? 1 : index + 1 });
}

/** How many slots a new macro could still take (empty, unused, not filled by the draft; deletions free one). */
export function freeSlots(state: KeysState, draft: MacrosDraft): number {
  let free = 0;
  for (const slot of state.macros) {
    const edit = editOf(draft, slot.slot);
    if (edit?.remove === true) free += 1;
    else if (slot.content.state === "empty" && slot.bindings.length === 0 && (edit?.program ?? null) === null) free += 1;
  }
  return free - draft.added.length;
}

/** Props of {@link SlotList}. */
export interface SlotListProps {
  state: KeysState;
  ctx: LabelContext;
  draft: MacrosDraft;
  target: Target | null;
  library: readonly LibraryMacroView[];
  editable: boolean;
  onOpen: (target: Target) => void;
  onNew: () => void;
  onAssign: (slot: number) => void;
  onRemove: (slot: number) => void;
  onKeep: (slot: number) => void;
  onRevert: (target: Target) => void;
  onSave: (program: MacroProgramView) => void;
}

function Warning({ children }: { children: string }) {
  return (
    <span className="keys-macros__warning ow-small">
      <Icon name="warning" size={14} />
      <span>{children}</span>
    </span>
  );
}

function SlotRow({ slot, props }: { slot: MacroSlotStateView; props: SlotListProps }) {
  const { state, ctx, draft, target, library, editable } = props;
  const dict = useDict();
  const t = dict.keysMacros.slots;
  const name = macroName(slot.slot, ctx, dict);
  const edit = editOf(draft, slot.slot);
  const draftProgram = edit?.remove === true ? null : (edit?.program ?? null);
  const current = slotProgram(slot);
  const shown = draftProgram ?? current;
  const open = target?.kind === "slot" && target.slot === slot.slot;
  const content = slot.content;
  const bound = slot.bindings.length > 0;
  const keys = bindingsText(slot.bindings, dict);

  let contentText: string = dict.keys.slot.empty;
  if (shown !== null) contentText = programText(shown, dict);
  else if (content.state === "kept_as_read") contentText = dict.keys.slot.keptAsRead;

  let usage = null;
  if (bound && content.state === "empty" && draftProgram === null) {
    usage = <Warning>{t.dangling({ keys, n: slot.bindings.length })}</Warning>;
  } else if (bound) {
    usage = <span className="ow-small">{t.runsFrom({ keys })}</span>;
  } else if (content.state !== "empty" && (edit?.bind ?? null) === null) {
    usage = <Warning>{t.orphan}</Warning>;
  }

  const tags: string[] = [];
  if (edit?.remove === true) tags.push(bound ? t.draft.deleted({ keys, n: slot.bindings.length }) : t.draft.removed);
  if (draftProgram !== null) tags.push(t.draft.changed);
  if (edit !== undefined && edit.bind !== null) tags.push(t.draft.bound({ key: bindText(edit.bind, state, dict) }));

  const actions = [];
  if (edit?.remove === true) {
    actions.push(
      <Button key="keep" size="sm" aria-label={t.actions.keep({ name })} onClick={() => { props.onKeep(slot.slot); }}>
        {t.labels.keep}
      </Button>,
    );
  } else {
    if (shown !== null) {
      actions.push(
        <Button key="edit" size="sm" aria-label={t.actions.edit({ name })} onClick={() => { props.onOpen({ kind: "slot", slot: slot.slot }); }}>
          {t.labels.edit}
        </Button>,
      );
    } else if (content.state === "kept_as_read") {
      actions.push(
        <Button key="replace" size="sm" disabled={!editable} aria-label={t.actions.replace({ name })} onClick={() => { props.onOpen({ kind: "slot", slot: slot.slot }); }}>
          {t.labels.replace}
        </Button>,
      );
    } else {
      // An empty Macro 1 no key runs: New macro… avoids the slot that empties the others.
      const slot0Free = slot.slot === 0 && !bound && (edit?.bind ?? null) === null;
      actions.push(
        <Button
          key="create"
          size="sm"
          disabled={!editable}
          disabledReason={slot0Free ? t.slot0.createInstead : null}
          aria-label={t.actions.create({ name })}
          onClick={() => {
            props.onOpen({ kind: "slot", slot: slot.slot });
          }}
        >
          {t.labels.create}
        </Button>,
      );
    }
    if (canBind(slot) && content.state !== "empty" && (edit?.bind ?? null) === null) {
      actions.push(
        <Button key="assign" size="sm" disabled={!editable} aria-label={t.actions.assign({ name })} onClick={() => { props.onAssign(slot.slot); }}>
          {t.labels.assign}
        </Button>,
      );
    }
    if (canRemove(slot)) {
      // One verb for "empty this slot in the draft"; the draft tag says what happens to its keys.
      actions.push(
        <Button key="delete" size="sm" variant="ghost" icon="trash" disabled={!editable} aria-label={t.actions.delete({ name })} onClick={() => { props.onRemove(slot.slot); }}>
          {t.labels.delete}
        </Button>,
      );
    }
    if (current !== null && content.state === "program" && libraryMatch(current, library, content.content_hash) === undefined) {
      actions.push(
        <Button key="save" size="sm" variant="ghost" aria-label={t.actions.save({ name })} onClick={() => { props.onSave(current); }}>
          {t.labels.save}
        </Button>,
      );
    }
    if (edit !== undefined) {
      actions.push(
        <Button key="undo" size="sm" variant="ghost" icon="undo" aria-label={t.actions.undo({ name })} onClick={() => { props.onRevert({ kind: "slot", slot: slot.slot }); }}>
          {t.labels.undo}
        </Button>,
      );
    }
  }

  return (
    <li
      className={`keys-macros__slot${open ? " keys-macros__slot--open" : ""}${edit === undefined ? "" : " keys-macros__slot--changed"}`}
      aria-current={open ? "true" : undefined}
    >
      <div className="keys-macros__slot-main">
        <span className="keys-macros__slot-name">
          <span className="ow-strong">{name}</span>
          {draftProgram === null && content.state === "program" ? <EvidenceBadge evidence={content.evidence} size="sm" /> : null}
          {draftProgram === null && content.state === "kept_as_read" ? <Badge kind="kept" size="sm" /> : null}
        </span>
        <span className="ow-small ow-muted">{contentText}</span>
        {usage}
        {tags.map((tag) => (
          <span key={tag} className="keys-macros__tag">
            {tag}
          </span>
        ))}
        {slot.slot === 0 && (bound || content.state !== "empty" || edit !== undefined) ? (
          <span className="ow-small ow-muted">{t.slot0.note}</span>
        ) : null}
      </div>
      <div className="keys-macros__slot-actions">{actions}</div>
    </li>
  );
}

function NewRow({ item, props }: { item: NewMacro; props: SlotListProps }) {
  const { state, draft, target } = props;
  const dict = useDict();
  const t = dict.keysMacros.slots;
  const name = newName(draft, item.id, dict);
  const open = target?.kind === "new" && target.id === item.id;
  // The key already runs a slot on its own: the library puts the new macro there.
  const own = ownMacroSlot(state, draft, item.bind);
  return (
    <li className={`keys-macros__slot keys-macros__slot--changed${open ? " keys-macros__slot--open" : ""}`} aria-current={open ? "true" : undefined}>
      <div className="keys-macros__slot-main">
        <span className="keys-macros__slot-name">
          <span className="ow-strong">{name}</span>
        </span>
        <span className="ow-small ow-muted">
          {item.program.steps.length === 0 ? dict.common.pickers.steps.empty : programText(item.program, dict)}
        </span>
        {item.bind === null ? <Warning>{t.noKeyYet}</Warning> : <span className="ow-small">{t.draft.bound({ key: bindText(item.bind, state, dict) })}</span>}
        <span className="ow-small ow-muted">{own === null ? t.newSlot : t.newSlotOwn({ n: own + 1 })}</span>
        <span className="keys-macros__tag">{t.draft.added}</span>
      </div>
      <div className="keys-macros__slot-actions">
        <Button size="sm" aria-label={t.actions.edit({ name })} onClick={() => { props.onOpen({ kind: "new", id: item.id }); }}>
          {t.labels.edit}
        </Button>
        <Button size="sm" variant="ghost" icon="trash" aria-label={t.actions.drop({ name })} onClick={() => { props.onRevert({ kind: "new", id: item.id }); }}>
          {t.labels.drop}
        </Button>
      </div>
    </li>
  );
}

/** The keyboard's macro slots and the draft's new macros. */
export function SlotList(props: SlotListProps) {
  const { state, draft, editable } = props;
  const dict = useDict();
  const t = dict.keysMacros.slots;
  const used = state.macros.filter((slot) => slot.content.state !== "empty").length;
  const free = freeSlots(state, draft);
  return (
    <Card
      title={t.title({ n: state.profile })}
      className="keys-macros__slots-card"
      actions={
        <Button
          size="sm"
          icon="plus"
          variant="primary"
          disabled={!editable}
          disabledReason={free > 0 ? null : t.noRoom}
          onClick={props.onNew}
        >
          {t.newMacro}
        </Button>
      }
    >
      <p className="ow-small ow-muted">{t.used({ n: used, total: state.limits.macro_slots })}</p>
      <p className="ow-small ow-muted">{t.newMacroHint}</p>
      <ul className="keys-macros__slots" aria-label={t.listLabel}>
        {state.macros.map((slot) => (
          <SlotRow key={slot.slot} slot={slot} props={props} />
        ))}
        {draft.added.map((item) => (
          <NewRow key={`new-${item.id}`} item={item} props={props} />
        ))}
      </ul>
    </Card>
  );
}
