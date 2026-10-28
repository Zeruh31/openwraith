// The key that runs a macro: the layer (Base / Fn, with the library's
// evidence of macros on that layer), the key (a KeyPicker over the W75's
// keys; keys that can't run a macro are shown with the reason), what the key
// does now, and the built-in notice when the key's Fn function would be
// replaced (a critical one needs the explicit confirmation, kept in the
// draft as the library's critical acknowledgement).

import { useState } from "react";

import { bindingEvidence } from "../../../components/actions";
import { EvidenceBadge } from "../../../components/Badge";
import { SegmentedControl } from "../../../components/Controls";
import { KeyPicker, type KeyOption } from "../../../components/KeyPicker";
import { keyName, useDict, type Dict } from "../../../i18n";
import type { BuiltinAckIntent, KeyAssignmentsView, KeysState, LayerView } from "../../../ipc/bindings";
import { BuiltinNotice, entryText, keyAt, type LabelContext } from "../shared";
import type { Binding } from "./model";

const LAYERS: readonly LayerView[] = ["base", "fn"];

/**
 * Why a key can't run a macro on a layer, or `null`. `editedSlots` (for a
 * new macro): the macro slots whose content the draft changes. A key that
 * runs one of them now can't take a new macro: the library would put the
 * new macro into that slot, over the drafted change.
 */
export function bindDisabledReason(
  key: KeyAssignmentsView,
  layer: LayerView,
  chosen: readonly Binding[],
  dict: Dict,
  editedSlots: readonly number[] = [],
): string | null {
  const t = dict.keysMacros.editor.keyDisabled;
  if (key.kind === "fn_key") return t.fnKey;
  if (key.kind === "knob") return t.knob;
  if (!key.macro_capable) return t.noUsage;
  if (layer === "fn" && key.fn_locked) return t.fnLocked;
  if (!(layer === "fn" ? key.assignable_fn : key.assignable_base)) return t.notAssignable;
  if (chosen.some((bind) => bind.layer === layer && bind.key === key.key.index)) return t.chosen;
  const origin = (layer === "fn" ? key.fn_layer : key.base).origin;
  if (origin.origin === "macro_slot" && editedSlots.includes(origin.slot)) return t.slotEdited({ n: origin.slot + 1 });
  return null;
}

/** The keys the picker offers on a layer. */
export function bindOptions(
  state: KeysState,
  layer: LayerView,
  chosen: readonly Binding[],
  dict: Dict,
  editedSlots: readonly number[] = [],
): KeyOption[] {
  return state.keys.map((key) => ({
    index: key.key.index,
    name: keyName(dict, key.key.name),
    legend: key.key.name,
    disabledReason: bindDisabledReason(key, layer, chosen, dict, editedSlots),
  }));
}

/** Props of {@link KeyBinding}. */
export interface KeyBindingProps {
  state: KeysState;
  ctx: LabelContext;
  value: Binding | null;
  onChange: (bind: Binding | null) => void;
  /** Keys other macros of the draft already take. */
  chosen: readonly Binding[];
  /** Critical built-ins the user confirmed. */
  acks: readonly BuiltinAckIntent[];
  onAck: (key: number, confirmed: boolean) => void;
  disabled?: boolean | undefined;
  /** For a new macro: the slots whose content the draft changes (their keys can't take it). */
  editedSlots?: readonly number[] | undefined;
}

/** Layer and key of a macro, with what the key does now and the built-in notice. */
export function KeyBinding({
  state,
  ctx,
  value,
  onChange,
  chosen,
  acks,
  onAck,
  disabled = false,
  editedSlots = [],
}: KeyBindingProps) {
  const dict = useDict();
  const t = dict.keysMacros.editor;
  const [pickedLayer, setPickedLayer] = useState<LayerView>(value?.layer ?? "base");
  const layer = value?.layer ?? pickedLayer;
  const options = bindOptions(state, layer, chosen, dict, editedSlots);
  const keyView = value === null ? undefined : keyAt(state, value.key);
  const entry = keyView === undefined ? undefined : layer === "fn" ? keyView.fn_layer : keyView.base;
  const evidence = bindingEvidence("macro", layer);

  const changeLayer = (next: LayerView) => {
    setPickedLayer(next);
    if (value === null) return;
    const key = keyAt(state, value.key);
    onChange(
      key !== undefined && bindDisabledReason(key, next, chosen, dict, editedSlots) === null ? { layer: next, key: value.key } : null,
    );
  };

  return (
    <div className="keys-macros__binding">
      <div className="keys-macros__binding-row">
        <div className="keys-macros__field">
          <span className="ow-field__label" aria-hidden="true">
            {t.layer}
          </span>
          <SegmentedControl<LayerView>
            label={t.layer}
            size="sm"
            options={LAYERS.map((item) => ({ value: item, label: dict.keys.layers[item] }))}
            value={layer}
            onChange={changeLayer}
            disabled={disabled}
          />
        </div>
        <EvidenceBadge evidence={evidence} size="sm" />
      </div>
      <KeyPicker
        label={t.key}
        value={value?.key ?? null}
        options={options}
        disabled={disabled}
        onChange={(index) => {
          onChange({ layer, key: index });
        }}
        description={
          entry === undefined || keyView === undefined ? undefined : t.now({ action: entryText(entry, keyView, ctx, dict) })
        }
      />
      {keyView !== undefined && layer === "fn" && keyView.builtin !== null ? (
        <BuiltinNotice
          keyView={keyView}
          profile={state.profile}
          confirmed={acks.some((ack) => ack.key === keyView.key.index && ack.critical)}
          onConfirm={(confirmed) => {
            onAck(keyView.key.index, confirmed);
          }}
        />
      ) : null}
    </div>
  );
}
