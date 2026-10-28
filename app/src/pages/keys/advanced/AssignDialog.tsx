// Assigning an advanced key no key uses (an orphan) to a key: the layer and
// the key (TargetKey, with the built-in notice of an Fn-layer key). Only
// puts the assignment into the tab's draft.

import { useState } from "react";

import { Button } from "../../../components/Button";
import { Dialog } from "../../../components/Dialog";
import { useDict } from "../../../i18n";
import type { BuiltinAckIntent, KeysState, LayerView } from "../../../ipc/bindings";
import { keyAt } from "../shared";
import { needsConfirmation, TargetKey, type TargetValue } from "./TargetKey";

/** Props of {@link AssignDialog}. */
export interface AssignDialogProps {
  state: KeysState;
  /** The slot's name ("Advanced key 4 · Toggle"). */
  name: string;
  onSave: (target: { layer: LayerView; key: number }, ack: BuiltinAckIntent | null) => void;
  onClose: () => void;
}

/** The assign dialog. Mount it to open it. */
export function AssignDialog({ state, name, onSave, onClose }: AssignDialogProps) {
  const dict = useDict();
  const t = dict.keysAdvanced;
  const [target, setTarget] = useState<TargetValue>({ layer: "base", key: null, confirmed: false });
  const reason =
    target.key === null ? t.editor.needTarget : needsConfirmation(state, target) ? t.editor.needConfirm : null;

  const save = () => {
    const key = target.key;
    if (reason !== null || key === null) return;
    const critical = target.layer === "fn" && keyAt(state, key)?.builtin?.critical === true;
    onSave({ layer: target.layer, key }, critical && target.confirmed ? { key, critical: true } : null);
  };

  return (
    <Dialog
      open
      size="md"
      title={t.assign.title({ name })}
      onClose={onClose}
      actions={
        <>
          <Button onClick={onClose}>{dict.common.actions.cancel}</Button>
          <Button variant="primary" disabledReason={reason} onClick={save}>
            {t.assign.confirm}
          </Button>
        </>
      }
    >
      <TargetKey state={state} value={target} onChange={setTarget} hint={t.assign.hint} />
    </Dialog>
  );
}
