// SOCD (`get_socd_state`, no keyboard traffic): the power of the whole
// keyboard and the pairs of the active profile, fetched again on the
// keyboard's own toggle (Fn + Right Shift); and the tab's draft (it survives
// leaving the tab; a draft made on another profile or connection is
// discarded).

import { api } from "../../../ipc/commands";
import { createPartStore } from "../shared/partStore";
import { createTabDraftStore } from "../shared/tabDraft";
import { EMPTY_SOCD, isSocdDirty, type SocdDraft } from "./model";

/** The SOCD state. */
export const socdPart = createPartStore(() => api.getSocdState());

/** The SOCD draft (the selection is not used). */
export const socdDrafts = createTabDraftStore<SocdDraft>({
  tab: "socd",
  empty: EMPTY_SOCD,
  isDirty: isSocdDirty,
  discardedText: (dict, profile, reconnect) =>
    reconnect ? dict.actuation.socd.discardedReconnect : dict.actuation.socd.discarded({ n: profile }),
});
