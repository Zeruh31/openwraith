// What the automatic profiles do and don't do: the "How it works"
// disclosure and the explanation shown before they are turned on, whose
// tick also stands for the "every wraith.software tab is closed"
// confirmation of automatic switches (they never ask per switch).

import { useState } from "react";

import { ConfirmDialog } from "../../../components/Dialog";
import { Checkbox } from "../../../components/Controls";
import { useDict } from "../../../i18n";

/** The points of the explanation. */
export function HowItWorksList() {
  const dict = useDict();
  const t = dict.settings.games.howItWorks;
  return (
    <ul className="st-games__how">
      <li>{t.writes}</li>
      <li>{t.waits}</li>
      <li>{t.stops}</li>
      <li>{t.manual}</li>
      <li>{t.exit}</li>
      <li>{t.limits}</li>
      <li>{t.privacy}</li>
    </ul>
  );
}

/** Props of {@link ConsentDialog}. */
export interface ConsentDialogProps {
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * The explanation before game profiles are turned on (mount it to show it).
 * "Turn on" needs the tick; Cancel has the focus, the button is armed after
 * 500 ms and Enter never confirms (ConfirmDialog).
 */
export function ConsentDialog({ onConfirm, onCancel }: ConsentDialogProps) {
  const dict = useDict();
  const t = dict.settings.games.consent;
  const [ticked, setTicked] = useState(false);
  return (
    <ConfirmDialog
      open
      title={t.title}
      confirmLabel={t.confirm}
      onConfirm={onConfirm}
      onCancel={onCancel}
      disabledReason={ticked ? null : t.needsTick}
      footer={<Checkbox checked={ticked} onChange={setTicked} label={t.tick} />}
    >
      <p>{t.intro}</p>
      <HowItWorksList />
    </ConfirmDialog>
  );
}
