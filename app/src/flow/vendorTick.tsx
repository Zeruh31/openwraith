// The "every wraith.software tab is closed" confirmation, shared by every
// dialog that leads to a write (WriteFlow's review and the restore's
// switch-consent dialog): the same note, the same checkbox, the same rule.
//
// The tick is required for the first write of an app run and again after
// every episode in which another program used the keyboard; a tick counts
// only within the episode it was given in.

import { useState } from "react";

import { Checkbox } from "../components/Controls";
import { useDict } from "../i18n";
import { useVendorTick } from "../state/session";

/** The state of the vendor confirmation in one dialog. */
export interface VendorConfirmation {
  /** The checkbox is shown (the next write needs it). */
  required: boolean;
  /** The user ticked it in the current episode. */
  checked: boolean;
  /** Required and not ticked: the action stays disabled. */
  missing: boolean;
  setChecked: (checked: boolean) => void;
  /** Records the tick for the run; call it when the action starts. */
  acknowledge: () => void;
}

/** The vendor confirmation of a dialog. */
export function useVendorConfirmation(): VendorConfirmation {
  const vendor = useVendorTick();
  const [tickedIn, setTickedIn] = useState<number | null>(null);
  const checked = tickedIn !== null && tickedIn === vendor.episode;
  return {
    required: vendor.required,
    checked,
    missing: vendor.required && !checked,
    setChecked: (value) => {
      setTickedIn(value ? vendor.episode : null);
    },
    acknowledge: vendor.acknowledge,
  };
}

/** The vendor-site note, with the checkbox when it is required. */
export function VendorNote({ confirmation }: { confirmation: VendorConfirmation }) {
  const dict = useDict();
  return (
    <>
      <p className="ow-muted">{dict.flow.review.vendorNote}</p>
      {confirmation.required ? (
        <Checkbox
          checked={confirmation.checked}
          onChange={confirmation.setChecked}
          label={dict.flow.review.vendorTick}
        />
      ) : null}
    </>
  );
}
