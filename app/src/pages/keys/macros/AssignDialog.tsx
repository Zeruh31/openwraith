// "Assign…" for a macro no key runs: choose the layer and the key; the
// choice goes into the draft (nothing is written until the review).

import { useState } from "react";

import { Button } from "../../../components/Button";
import { Dialog } from "../../../components/Dialog";
import { useDict } from "../../../i18n";
import type { BuiltinAckIntent, KeysState } from "../../../ipc/bindings";
import type { LabelContext } from "../shared";
import { KeyBinding } from "./KeyBinding";
import type { Binding } from "./model";

/** Props of {@link AssignDialog}. */
export interface AssignDialogProps {
  state: KeysState;
  ctx: LabelContext;
  /** The macro's name ("Macro 4"). */
  name: string;
  chosen: readonly Binding[];
  acks: readonly BuiltinAckIntent[];
  onAck: (key: number, confirmed: boolean) => void;
  onAssign: (bind: Binding) => void;
  onClose: () => void;
}

/** Gives a macro no key runs a key. Mount it to open it. */
export function AssignDialog({ state, ctx, name, chosen, acks, onAck, onAssign, onClose }: AssignDialogProps) {
  const dict = useDict();
  const t = dict.keysMacros.assign;
  const [bind, setBind] = useState<Binding | null>(null);
  return (
    <Dialog
      open
      title={t.title({ name })}
      onClose={onClose}
      actions={
        <>
          <Button onClick={onClose}>{dict.common.actions.cancel}</Button>
          <Button
            variant="primary"
            disabledReason={bind === null ? t.needKey : null}
            onClick={() => {
              if (bind !== null) onAssign(bind);
            }}
          >
            {t.confirm}
          </Button>
        </>
      }
    >
      <div className="ow-stack">
        <p>{t.intro}</p>
        <KeyBinding state={state} ctx={ctx} value={bind} onChange={setBind} chosen={chosen} acks={acks} onAck={onAck} />
      </div>
    </Dialog>
  );
}
