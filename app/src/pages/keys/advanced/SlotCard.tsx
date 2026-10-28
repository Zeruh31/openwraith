// The cards of the Advanced keys tab: one per used slot (with the draft's
// change of it: changed, assigned, deleted) and one per new advanced key of
// the draft. A card says in words what the advanced key does, which keys use
// it (or that none does), its evidence ("Not yet verified" from the DTO,
// "Kept as read" for a kind OpenWraith can't read), and the bytes and slot
// numbers under "Technical details". Buttons only change the draft.

import { combineEvidence } from "../../../components/actions";
import { Badge, EvidenceBadge } from "../../../components/Badge";
import { Button } from "../../../components/Button";
import { Icon } from "../../../components/Icon";
import { TechnicalDetails } from "../../../components/Layout";
import { useDict, useLang, type Dict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type { AdvancedDefView, BindingView, KeysState, LayerView } from "../../../ipc/bindings";
import { entryText, keyAt, keyOnLayer } from "../shared";
import {
  defEvidence,
  defSentence,
  layerBindingEvidence,
  slotFor,
  usedRows,
  type DraftItem,
  type SlotCardModel,
} from "./model";

/** The technical lines of a definition. */
function defTechnical(def: AdvancedDefView, dict: Dict): string[] {
  const t = dict.keysAdvanced.card.technical;
  const hex = (usage: number) => `0x${usage.toString(16).padStart(2, "0")}`;
  switch (def.type) {
    case "dks":
      return [
        t.firstPoint({ value: def.first_point }),
        ...usedRows(def.rows).map(({ position, row }) =>
          t.row({ n: position + 1, usage: hex(row.key), steps: row.steps.join("/") }),
        ),
      ];
    case "mod_tap":
      return [`${hex(def.tap)} / ${hex(def.hold)}`, t.holdTime({ value: def.hold_ms })];
    case "toggle":
      return [hex(def.key)];
  }
}

/** The keys that use a slot, each with its entry's evidence (an advanced key on the Fn layer isn't verified yet). */
function Bindings({ bindings, state }: { bindings: readonly BindingView[]; state: KeysState }) {
  const dict = useDict();
  if (bindings.length === 0) {
    return (
      <p className="keys-adv-card__orphan ow-small">
        <Icon name="warning" size={16} />
        <span>{dict.keysAdvanced.card.orphan}</span>
      </p>
    );
  }
  return (
    <p className="keys-adv-card__bindings ow-small">
      <span className="ow-muted">{dict.keysAdvanced.card.usedBy}</span>
      {bindings.map((binding) => {
        const key = keyAt(state, binding.key.index);
        const entry = key === undefined ? null : binding.layer === "base" ? key.base : key.fn_layer;
        return (
          <span key={`${binding.layer}-${binding.key.index}`} className="keys-adv-card__binding">
            <span className="ow-strong">{keyOnLayer(binding.key.name, binding.layer, dict)}</span>
            <EvidenceBadge evidence={entry?.evidence ?? null} size="sm" />
          </span>
        );
      })}
    </p>
  );
}

/** Props of {@link SlotCard}. */
export interface SlotCardProps {
  model: SlotCardModel;
  state: KeysState;
  /** Edits are possible (else the buttons are not offered). */
  canEdit: boolean;
  onEdit: (slot: number, def: AdvancedDefView, itemId: string | null) => void;
  onAssign: (slot: number) => void;
  onDelete: (slot: number) => void;
  onDrop: (itemId: string) => void;
}

/** A used slot's card. */
export function SlotCard({ model, state, canEdit, onEdit, onAssign, onDelete, onDrop }: SlotCardProps) {
  const dict = useDict();
  const lang = useLang();
  const format = useFormat();
  const list = (items: readonly string[]) => format.list(items);
  const t = dict.keysAdvanced.card;
  const { slot, change } = model;
  const content = slot.content;
  const n = slot.slot + 1;
  const name =
    content.state === "def"
      ? dict.keys.action.advancedTyped({ n, type: dict.keys.advancedTypes[content.def.type] })
      : dict.keys.action.advanced({ n });
  const titleId = `keys-adv-slot-${slot.slot}`;
  const drafted = change?.kind === "edit" ? change.def : null;
  const shown = drafted ?? (content.state === "def" ? content.def : null);
  const slotEvidence = content.state === "def" ? content.evidence : null;
  const technical = [t.technical.slot({ slot: slot.slot })];
  if (content.state === "kept_as_read") {
    technical.push(t.technical.kept({ type: String(content.type_byte), hex: content.hex }));
  } else if (shown !== null) {
    technical.push(...defTechnical(shown, dict));
  }

  let tag: string | null = null;
  if (change?.kind === "edit") tag = t.tags.changed;
  if (change?.kind === "delete") tag = t.tags.deleted;
  if (change?.kind === "assign") {
    const key = keyAt(state, change.key);
    tag = t.tags.assigned({ key: keyOnLayer(key?.key.name ?? String(change.key), change.layer, dict) });
  }

  return (
    <article
      className={`keys-adv-card${change === null ? "" : " keys-adv-card--drafted"}${change?.kind === "delete" ? " keys-adv-card--deleted" : ""}`}
      aria-labelledby={titleId}
    >
      <header className="keys-adv-card__header">
        <h3 id={titleId} className="keys-adv-card__title">
          {name}
        </h3>
        <span className="keys-adv-card__badges">
          {tag === null ? null : <span className="keys-adv-tag">{tag}</span>}
          {content.state === "kept_as_read" ? (
            <Badge kind="kept" size="sm" />
          ) : (
            <EvidenceBadge evidence={drafted === null ? slotEvidence : defEvidence(drafted)} size="sm" />
          )}
        </span>
      </header>
      {content.state === "kept_as_read" ? (
        <p className="ow-small">{t.kept({ type: content.type_byte })}</p>
      ) : shown === null ? null : (
        <p className="keys-adv-card__sentence">{defSentence(shown, dict, lang, list)}</p>
      )}
      {drafted !== null && content.state === "def" ? (
        <p className="ow-small ow-muted">{t.now({ text: defSentence(content.def, dict, lang, list) })}</p>
      ) : null}
      <Bindings bindings={slot.bindings} state={state} />
      {change?.kind === "delete" && slot.bindings.length > 0 ? (
        <p className="ow-small">
          {t.deleteNote({ keys: slot.bindings.map((b) => keyOnLayer(b.key.name, b.layer, dict)).join(", ") })}
        </p>
      ) : null}
      <TechnicalDetails>{technical.join("\n")}</TechnicalDetails>
      {canEdit ? (
        <div className="keys-adv-card__actions">
          {change !== null ? (
            <Button
              size="sm"
              icon="undo"
              aria-label={change.kind === "delete" ? t.keepNamed({ name }) : t.dropNamed({ name })}
              onClick={() => {
                onDrop(change.id);
              }}
            >
              {change.kind === "delete" ? t.keep : t.drop}
            </Button>
          ) : null}
          {change?.kind !== "delete" && content.state === "def" ? (
            <Button
              size="sm"
              aria-label={t.editNamed({ name })}
              onClick={() => {
                onEdit(slot.slot, drafted ?? content.def, change?.kind === "edit" ? change.id : null);
              }}
            >
              {t.edit}
            </Button>
          ) : null}
          {change === null && content.state === "def" && slot.bindings.length === 0 ? (
            <Button
              size="sm"
              aria-label={t.assignNamed({ name })}
              onClick={() => {
                onAssign(slot.slot);
              }}
            >
              {t.assign}
            </Button>
          ) : null}
          {change?.kind !== "delete" ? (
            <Button
              size="sm"
              variant="ghost"
              icon="trash"
              aria-label={t.deleteNamed({ name })}
              onClick={() => {
                onDelete(slot.slot);
              }}
            >
              {t.delete}
            </Button>
          ) : null}
        </div>
      ) : null}
    </article>
  );
}

/** Props of {@link NewCard}. */
export interface NewCardProps {
  item: Extract<DraftItem, { kind: "create" }>;
  state: KeysState;
  canEdit: boolean;
  onEdit: (item: Extract<DraftItem, { kind: "create" }>) => void;
  onDrop: (itemId: string) => void;
}

/** The evidence of a new advanced key: its definition and its layer's binding rule. */
export function newKeyEvidence(def: AdvancedDefView, layer: LayerView) {
  const parts = [defEvidence(def), layerBindingEvidence(layer)];
  return parts.every((part) => part === null) ? null : combineEvidence(...parts);
}

/** A new advanced key of the draft. */
export function NewCard({ item, state, canEdit, onEdit, onDrop }: NewCardProps) {
  const dict = useDict();
  const lang = useLang();
  const format = useFormat();
  const list = (items: readonly string[]) => format.list(items);
  const t = dict.keysAdvanced.card;
  const key = keyAt(state, item.key);
  const keyText = keyOnLayer(key?.key.name ?? String(item.key), item.layer, dict);
  const typeName = dict.keys.advancedTypes[item.def.type];
  const titleId = `keys-adv-new-${item.id}`;
  const slot = slotFor(state, item.layer, item.key);
  const technical = [
    slot === null ? t.technical.noneFree : t.technical.chosen({ slot }),
    ...defTechnical(item.def, dict),
  ];
  const entry = key === undefined ? null : item.layer === "base" ? key.base : key.fn_layer;
  const name = `${typeName} · ${keyText}`;

  return (
    <article className="keys-adv-card keys-adv-card--drafted keys-adv-card--new" aria-labelledby={titleId}>
      <header className="keys-adv-card__header">
        <h3 id={titleId} className="keys-adv-card__title">
          {typeName}
        </h3>
        <span className="keys-adv-card__badges">
          <span className="keys-adv-tag">{t.tags.new}</span>
          <EvidenceBadge evidence={newKeyEvidence(item.def, item.layer)} size="sm" />
        </span>
      </header>
      <p className="ow-strong ow-small">{t.newOn({ key: keyText })}</p>
      <p className="keys-adv-card__sentence">{defSentence(item.def, dict, lang, list)}</p>
      {entry === null || key === undefined ? null : (
        <p className="ow-small ow-muted">{t.now({ text: entryText(entry, key, { state }, dict) })}</p>
      )}
      <TechnicalDetails>{technical.join("\n")}</TechnicalDetails>
      {canEdit ? (
        <div className="keys-adv-card__actions">
          <Button
            size="sm"
            icon="undo"
            aria-label={t.dropNamed({ name })}
            onClick={() => {
              onDrop(item.id);
            }}
          >
            {t.drop}
          </Button>
          <Button
            size="sm"
            aria-label={t.editNamed({ name })}
            onClick={() => {
              onEdit(item);
            }}
          >
            {t.edit}
          </Button>
        </div>
      ) : null}
    </article>
  );
}
