// What replacing a key's Fn built-in means, shown before the change goes
// into the draft: a warning for an ordinary built-in (F1-F12 media keys,
// lighting keys, …), and for the essential ones (Fn + 1..4 switch profiles,
// Fn + Space resets) a danger callout with an explicit ConfirmCheckbox whose
// tick becomes the library's critical acknowledgement. Replacing Fn + Space
// has not been tested, and the text says so.

import { Badge } from "../../../components/Badge";
import { ConfirmCheckbox } from "../../../components/ConfirmCheckbox";
import { Callout } from "../../../components/Feedback";
import { builtinName, keyRefText } from "../../../flow/labels";
import { useDict } from "../../../i18n";
import type { KeyAssignmentsView } from "../../../ipc/bindings";
import { criticalBuiltinCallout, profileOfBuiltin } from "./KeymapReview";

/** Props of {@link BuiltinNotice}. */
export interface BuiltinNoticeProps {
  /** The key whose Fn-layer entry would change (nothing is shown without a built-in). */
  keyView: KeyAssignmentsView;
  /** The active profile (named in the consequence). */
  profile: number;
  /** For a critical built-in: whether the user confirmed. */
  confirmed: boolean;
  onConfirm: (confirmed: boolean) => void;
}

/** The built-in warning (and, for a critical one, its confirmation). */
export function BuiltinNotice({ keyView, profile, confirmed, onConfirm }: BuiltinNoticeProps) {
  const dict = useDict();
  const builtin = keyView.builtin;
  if (builtin === null) return null;
  const name = builtinName(builtin.index, builtin.function, dict);
  const key = keyRefText(keyView.key, "fn", dict);
  const unconfirmedLabel = builtin.label_confirmed ? null : (
    <Badge kind="unverified" size="sm" tooltip={dict.keys.builtinLabelUnconfirmed} />
  );
  if (!builtin.critical) {
    return (
      <Callout kind="warning">
        <p>
          {dict.keys.builtinWarning({ key, function: name })} {unconfirmedLabel}
        </p>
      </Callout>
    );
  }
  const k = profileOfBuiltin(keyView.key.name);
  // Fn + k on profile k switches nowhere: the confirmation stays, but it is no danger.
  const tone = criticalBuiltinCallout(keyView.key.name, profile, dict);
  return (
    <Callout kind={tone.kind} title={tone.title}>
      <p>
        {k === null ? dict.keys.builtinReset : dict.keys.builtinProfile({ k, n: profile, likely: !builtin.label_confirmed })}
      </p>
      <ConfirmCheckbox
        checked={confirmed}
        tone={tone.kind}
        onChange={onConfirm}
        label={k === null ? dict.keys.builtinConfirmReset({ n: profile }) : dict.keys.builtinConfirmProfile({ k, n: profile })}
      />
    </Callout>
  );
}
