// Keys > Assignments: "Swap with…". The selected key and another key on the
// same layer exchange what they do (after the draft's own changes). Keys
// whose entry is kept as read can't be moved, and an essential Fn built-in
// (Fn + 1..4, Fn + Space) can only be replaced after its own confirmation,
// so those keys are offered with the reason instead. Swapping only changes
// the draft.

import { useState } from "react";

import { Button } from "../../../components/Button";
import { Dialog } from "../../../components/Dialog";
import { KeyPicker, type KeyOption } from "../../../components/KeyPicker";
import { keyName, useDict } from "../../../i18n";
import type { KeyAssignmentsView, KeysState, LayerView } from "../../../ipc/bindings";
import { keyOnLayer, type LabelContext } from "../shared";
import { capLegend, valueText } from "./caps";
import { canSwap, effectiveValue, isAssignable, isConfirmed, isCritical, type AssignDraft } from "./model";

/** Props of {@link SwapDialog}. */
export interface SwapDialogProps {
  state: KeysState;
  draft: AssignDraft;
  layer: LayerView;
  /** The selected key. */
  keyView: KeyAssignmentsView;
  labels: LabelContext;
  onSwap: (other: number) => void;
  onClose: () => void;
}

/** The swap dialog (mount it to open it). */
export function SwapDialog({ state, draft, layer, keyView, labels, onSwap, onClose }: SwapDialogProps) {
  const dict = useDict();
  const t = dict.keysAssign.swap;
  const [other, setOther] = useState<number | null>(null);
  const index = keyView.key.index;

  const options: KeyOption[] = state.keys
    .filter((key) => key.kind === "normal" && key.key.index !== index && isAssignable(key, layer))
    .map((key) => {
      const value = effectiveValue(state, draft, layer, key.key.index);
      let disabledReason: string | null = null;
      if (value === null) disabledReason = t.reasons.kept;
      else if (isCritical(key, layer) && !isConfirmed(draft, key.key.index)) disabledReason = t.reasons.critical;
      else if (!canSwap(state, draft, layer, index, key.key.index)) disabledReason = t.reasons.same;
      return {
        index: key.key.index,
        name: keyName(dict, key.key.name),
        legend: capLegend(dict, key.key.name),
        disabledReason,
      };
    });

  const partner = other === null ? undefined : state.keys.find((key) => key.key.index === other);
  const mine = effectiveValue(state, draft, layer, index);
  const theirs = other === null ? null : effectiveValue(state, draft, layer, other);
  const result =
    partner === undefined || mine === null || theirs === null
      ? null
      : t.result({
          a: keyOnLayer(keyView.key.name, layer, dict),
          aNew: valueText(theirs, keyView, layer, labels, dict),
          b: keyOnLayer(partner.key.name, layer, dict),
          bNew: valueText(mine, partner, layer, labels, dict),
        });

  return (
    <Dialog
      open
      title={t.title({ key: keyOnLayer(keyView.key.name, layer, dict) })}
      onClose={onClose}
      actions={
        <>
          <Button onClick={onClose}>{dict.common.actions.cancel}</Button>
          <Button
            variant="primary"
            disabledReason={other === null ? t.pickKey : null}
            onClick={() => {
              if (other !== null) onSwap(other);
            }}
          >
            {t.confirm}
          </Button>
        </>
      }
    >
      <div className="ow-stack">
        <p className="ow-muted">{t.intro}</p>
        <KeyPicker label={t.label} value={other} onChange={setOther} options={options} />
        {result === null ? null : (
          <p role="status" className="ow-strong">
            {result}
          </p>
        )}
      </div>
    </Dialog>
  );
}
