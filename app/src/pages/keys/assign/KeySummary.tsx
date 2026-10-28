// Keys > Assignments: the selected key. What it does on the shown layer and
// what kind of entry that is (default, changed, a macro or an advanced key
// with its slot, kept as read), the keyboard's own Fn function, what it does
// on the other layer and, for a drafted change, what it will do after
// writing. The actions only change the draft: Change… (the action picker),
// Reset to default, Swap with… and "Keep what the keyboard has".
//
// Replacing a built-in Fn function shows its warning here; the essential
// ones (Fn + 1..4, Fn + Space) need the explicit ConfirmCheckbox first,
// which becomes the library's critical acknowledgement. Bytes are under
// "Technical details".

import { useId } from "react";

import { Badge, EvidenceBadge } from "../../../components/Badge";
import { Button } from "../../../components/Button";
import { Callout } from "../../../components/Feedback";
import { Card, KeyValueList, TechnicalDetails, type KeyValueItem } from "../../../components/Layout";
import { builtinName, keyRefText } from "../../../flow/labels";
import { useDict } from "../../../i18n";
import type { BrokenReferenceView, KeyAssignmentsView, KeysState, LayerView } from "../../../ipc/bindings";
import { href, tabTarget } from "../../../routes";
import {
  advancedDefText,
  BuiltinNotice,
  entryText,
  keyOnLayer,
  programText,
  type LabelContext,
} from "../shared";
import { brokenText } from "./BrokenReferences";
import { notAssignableText, valueText } from "./caps";
import {
  entryOn,
  isConfirmed,
  isCritical,
  targetOf,
  type AssignDraft,
} from "./model";

/** Props of {@link KeySummary}. */
export interface KeySummaryProps {
  state: KeysState;
  draft: AssignDraft;
  layer: LayerView;
  /** The selected key, if any. */
  keyView: KeyAssignmentsView | undefined;
  labels: LabelContext;
  /** Why the draft can't be changed now (keys not editable, values out of date), or `null`. */
  editBlock: string | null;
  onChange: () => void;
  onReset: () => void;
  onSwap: () => void;
  onKeep: () => void;
  /** Resets a key that refers to an empty slot. */
  onResetBroken: (item: BrokenReferenceView) => void;
  onConfirm: (confirmed: boolean) => void;
}

const OTHER: Record<LayerView, LayerView> = { base: "fn", fn: "base" };

/** The selected key's summary and actions. */
export function KeySummary({
  state,
  draft,
  layer,
  keyView,
  labels,
  editBlock,
  onChange,
  onReset,
  onSwap,
  onKeep,
  onResetBroken,
  onConfirm,
}: KeySummaryProps) {
  const dict = useDict();
  const t = dict.keysAssign;
  const noteId = useId();
  if (keyView === undefined) {
    return (
      <Card title={t.summary.title} className="keys-assign-summary">
        <p className="ow-muted">{t.summary.none}</p>
      </Card>
    );
  }

  const index = keyView.key.index;
  const entry = entryOn(keyView, layer);
  const other = OTHER[layer];
  const otherEntry = entryOn(keyView, other);
  const target = targetOf(draft, layer, index);
  const effective = target?.action ?? entry.value;
  const blocked = notAssignableText(keyView, layer, dict);
  const critical = isCritical(keyView, layer);
  const confirmed = isConfirmed(draft, index);
  const builtin = layer === "fn" ? keyView.builtin : null;
  const broken = state.broken.find((item) => item.layer === layer && item.key.index === index);
  const title = keyOnLayer(keyView.key.name, layer, dict);

  const items: KeyValueItem[] = [
    {
      key: "does",
      label: t.summary.does,
      value: (
        <span className="keys-entry">
          <span>{entryText(entry, keyView, labels, dict)}</span>
          <EvidenceBadge evidence={entry.evidence} keptWhenNone={entry.preserved !== null} size="sm" />
        </span>
      ),
    },
    { key: "origin", label: t.summary.origin, value: dict.keys.origin[entry.origin.origin] },
  ];

  const origin = entry.origin;
  if (origin.origin === "macro_slot") {
    const content = state.macros.find((slot) => slot.slot === origin.slot)?.content;
    items.push({
      key: "slot",
      label: t.summary.slot,
      value: (
        <span className="keys-assign-summary__slot">
          <span className="keys-entry">
            <span>
              {content === undefined || content.state === "empty"
                ? t.summary.emptySlot
                : content.state === "program"
                  ? programText(content.program, dict)
                  : dict.keys.slot.keptAsRead}
            </span>
            {content?.state === "program" ? <EvidenceBadge evidence={content.evidence} size="sm" /> : null}
            {content?.state === "kept_as_read" ? <Badge kind="kept" size="sm" /> : null}
          </span>
          <a className="ow-small" href={href(tabTarget("keys", "macros"))}>
            {t.summary.openMacros}
          </a>
        </span>
      ),
    });
  } else if (origin.origin === "advanced_slot") {
    const content = state.advanced.find((slot) => slot.slot === origin.slot)?.content;
    items.push({
      key: "slot",
      label: t.summary.slot,
      value: (
        <span className="keys-assign-summary__slot">
          <span className="keys-entry">
            <span>
              {content === undefined || content.state === "empty"
                ? t.summary.emptySlot
                : content.state === "def"
                  ? advancedDefText(content.def, dict)
                  : dict.keys.slot.keptAsRead}
            </span>
            {content?.state === "def" ? <EvidenceBadge evidence={content.evidence} size="sm" /> : null}
            {content?.state === "kept_as_read" ? <Badge kind="kept" size="sm" /> : null}
          </span>
          <a className="ow-small" href={href(tabTarget("keys", "advanced"))}>
            {t.summary.openAdvanced}
          </a>
        </span>
      ),
    });
  }
  if (builtin !== null) {
    items.push({
      key: "builtin",
      label: t.summary.builtin,
      value: (
        <span className="keys-entry">
          <span>{builtinName(builtin.index, builtin.function, dict)}</span>
          {builtin.label_confirmed ? null : (
            <Badge kind="unverified" size="sm" tooltip={dict.keys.builtinLabelUnconfirmed} />
          )}
        </span>
      ),
    });
  }
  items.push({
    key: "other",
    label: t.summary.otherLayer[other],
    value: (
      <span className="keys-entry">
        <span>{entryText(otherEntry, keyView, labels, dict)}</span>
        <EvidenceBadge evidence={otherEntry.evidence} keptWhenNone={otherEntry.preserved !== null} size="sm" />
      </span>
    ),
  });
  if (target !== undefined) {
    items.push({
      key: "after",
      label: t.summary.after,
      value: (
        <span className="keys-entry ow-strong keys-assign-summary__after">
          <span aria-hidden="true">●</span>
          <span>{valueText(target.action, keyView, layer, labels, dict)}</span>
          <span className="sr-only">{t.legend.pending}</span>
        </span>
      ),
    });
  }

  // Built-in Fn functions: the essential ones always ask for the explicit
  // confirmation; an ordinary one warns while it is active, or says that it
  // is already replaced.
  let builtinNote = null;
  if (builtin !== null && blocked === null) {
    if (critical || entry.origin.origin === "default") {
      builtinNote = (
        <BuiltinNotice keyView={keyView} profile={state.profile} confirmed={confirmed} onConfirm={onConfirm} />
      );
    } else {
      builtinNote = (
        <Callout kind="info">
          <p>
            {t.summary.builtinReplaced({
              key: keyRefText(keyView.key, "fn", dict),
              function: builtinName(builtin.index, builtin.function, dict),
            })}
          </p>
        </Callout>
      );
    }
  }

  const allBlocked = blocked ?? editBlock;
  const needsConfirm = critical && !confirmed;
  const note = allBlocked ?? (needsConfirm ? t.actions.reasons.confirmFirst : null);
  const isDefault = effective?.kind === "default";
  const technical = [
    t.summary.bytes({ layer: dict.keys.layers[layer], hex: entry.hex }),
    t.summary.bytes({ layer: dict.keys.layers[other], hex: otherEntry.hex }),
    ...(entry.preserved === null ? [] : [t.summary.keptText({ text: entry.preserved.text })]),
  ].join("\n");

  return (
    <Card
      title={t.summary.title}
      className="keys-assign-summary"
      actions={<span className="ow-strong keys-assign-summary__key">{title}</span>}
    >
      <div className="ow-stack">
        <KeyValueList items={items} />
        {blocked === null ? null : <Callout kind="info">{blocked}</Callout>}
        {broken === undefined || target !== undefined ? null : (
          <Callout
            kind="warning"
            actions={
              broken.resettable && allBlocked === null ? (
                <Button
                  size="sm"
                  onClick={() => {
                    onResetBroken(broken);
                  }}
                >
                  {t.broken.reset}
                </Button>
              ) : undefined
            }
          >
            <p>{brokenText(broken, dict)}</p>
          </Callout>
        )}
        {builtinNote}
        {blocked === null ? (
          <div className="keys-assign-summary__actions" role="group" aria-label={t.actions.label}>
            <Button
              variant="primary"
              icon="keys"
              disabled={allBlocked !== null || needsConfirm}
              aria-describedby={note === null ? undefined : noteId}
              onClick={onChange}
            >
              {t.actions.change}
            </Button>
            <Button
              disabled={allBlocked !== null}
              aria-describedby={allBlocked === null ? undefined : noteId}
              disabledReason={allBlocked === null && isDefault ? t.actions.reasons.alreadyDefault : null}
              onClick={onReset}
            >
              {t.actions.reset}
            </Button>
            <Button
              disabled={allBlocked !== null || needsConfirm}
              aria-describedby={note === null || effective === null ? undefined : noteId}
              disabledReason={allBlocked === null && effective === null ? t.actions.reasons.keptAsRead : null}
              onClick={onSwap}
            >
              {t.actions.swap}
            </Button>
            {target === undefined ? null : (
              <Button variant="ghost" disabled={allBlocked !== null} onClick={onKeep}>
                {t.actions.keep}
              </Button>
            )}
          </div>
        ) : null}
        {note === null || blocked !== null ? null : (
          <p id={noteId} className="ow-small ow-muted">
            {note}
          </p>
        )}
        <TechnicalDetails>{technical}</TechnicalDetails>
      </div>
    </Card>
  );
}
