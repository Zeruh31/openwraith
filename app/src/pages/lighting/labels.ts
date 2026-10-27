// Words for the Lighting page that depend on values (effect names, the
// brightness text, the apply bar's disabled reason).

import type { Dict } from "../../i18n";
import type { LightModeView } from "../../ipc/bindings";
import { hexId, type ApplyBlock } from "./model";

/** The localized name of an effect; an unknown effect is named with its id ("Unknown effect (0x05)"). */
export function modeName(dict: Dict, mode: LightModeView): string {
  return mode.key === "unknown" ? dict.lighting.effects.unknown({ id: hexId(mode.id) }) : dict.lighting.modes[mode.key];
}

/** The apply bar's disabled reason, or `null`. */
export function applyBlockText(dict: Dict, block: ApplyBlock | null): string | null {
  if (block === null) return null;
  const t = dict.lighting.applyBar;
  switch (block.kind) {
    case "not_ready":
      return t.notReady;
    case "busy":
      return t.busy;
    case "gate":
      return dict.errors.writeBlocked[block.reason];
    case "not_editable":
      return t.notEditable;
    case "choose_effect":
      return t.chooseEffect;
  }
}
