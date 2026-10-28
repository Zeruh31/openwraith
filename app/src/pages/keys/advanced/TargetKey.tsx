// The key an advanced key goes on: the layer (Base / Fn, with the
// catalogue's evidence of advanced keys on that layer), the key (a combobox
// plus "Pick on the board…"), what the key does now, and the built-in
// notice of an Fn-layer key (a critical built-in needs its ConfirmCheckbox
// before the change can go into the draft).

import { EvidenceBadge } from "../../../components/Badge";
import { SegmentedControl } from "../../../components/Controls";
import { KeyPicker, type KeyOption } from "../../../components/KeyPicker";
import { keyName, useDict, type Dict } from "../../../i18n";
import type { KeyAssignmentsView, KeysState, LayerView } from "../../../ipc/bindings";
import { BuiltinNotice, entryText, keyAt } from "../shared";
import { layerBindingEvidence, targetBlock } from "./model";

/** The chosen layer and key, and whether a critical built-in was confirmed. */
export interface TargetValue {
  layer: LayerView;
  key: number | null;
  /** The critical built-in of the key was confirmed (ConfirmCheckbox). */
  confirmed: boolean;
}

/** The keys a layer offers, with the reason a key can't take an advanced key. */
export function targetOptions(state: KeysState, layer: LayerView, dict: Dict): KeyOption[] {
  const t = dict.keysAdvanced.editor.blocked;
  return state.keys.map((key) => {
    const block = targetBlock(key, layer);
    return {
      index: key.key.index,
      name: keyName(dict, key.key.name),
      disabledReason: block === null ? null : t[block],
    };
  });
}

/** Whether the target needs a confirmation it hasn't got (a critical Fn built-in). */
export function needsConfirmation(state: KeysState, target: TargetValue): boolean {
  if (target.layer !== "fn" || target.key === null) return false;
  return keyAt(state, target.key)?.builtin?.critical === true && !target.confirmed;
}

/** The entry of a key on a layer. */
function entryOn(key: KeyAssignmentsView, layer: LayerView) {
  return layer === "base" ? key.base : key.fn_layer;
}

/** Props of {@link TargetKey}. */
export interface TargetKeyProps {
  state: KeysState;
  value: TargetValue;
  onChange: (value: TargetValue) => void;
  /** The label of the key field (default: "Key on the keyboard"). */
  label?: string | undefined;
  hint?: string | undefined;
  disabled?: boolean | undefined;
}

/** The layer and key fields. */
export function TargetKey({ state, value, onChange, label, hint, disabled = false }: TargetKeyProps) {
  const dict = useDict();
  const t = dict.keysAdvanced.editor;
  const options = targetOptions(state, value.layer, dict);
  const key = value.key === null ? undefined : keyAt(state, value.key);
  const layerEvidence = layerBindingEvidence(value.layer);

  const setLayer = (layer: LayerView) => {
    const keep = key !== undefined && targetBlock(key, layer) === null;
    onChange({ layer, key: keep ? key.key.index : null, confirmed: false });
  };

  return (
    <div className="keys-adv-target ow-stack">
      <div className="keys-adv-target__layer">
        <span className="ow-field__label">{t.layer}</span>
        <div className="keys-adv-target__layer-row">
          <SegmentedControl<LayerView>
            label={t.layer}
            size="sm"
            disabled={disabled}
            options={[
              { value: "base", label: dict.keys.layers.base },
              { value: "fn", label: dict.keys.layers.fn },
            ]}
            value={value.layer}
            onChange={setLayer}
          />
          <EvidenceBadge evidence={layerEvidence} size="sm" />
        </div>
      </div>
      <KeyPicker
        label={label ?? t.target}
        value={value.key}
        options={options}
        disabled={disabled}
        description={hint ?? t.targetHint}
        onChange={(index) => {
          onChange({ layer: value.layer, key: index, confirmed: false });
        }}
      />
      {key === undefined ? null : (
        <p className="ow-small">
          {t.now({ text: entryText(entryOn(key, value.layer), key, { state }, dict) })}
        </p>
      )}
      {key === undefined || value.layer !== "fn" ? null : (
        <BuiltinNotice
          keyView={key}
          profile={state.profile}
          confirmed={value.confirmed}
          onConfirm={(confirmed) => {
            onChange({ ...value, confirmed });
          }}
        />
      )}
    </div>
  );
}
