// The dead band of the active profile (`get_dead_band_state`, no keyboard
// traffic) and the tab's draft and selection (they survive leaving the tab;
// a draft made on another profile or connection is discarded).

import { api } from "../../../ipc/commands";
import { createPartStore } from "../shared/partStore";
import { createTabDraftStore } from "../shared/tabDraft";
import { EMPTY_DEAD_BAND, isDeadBandDirty, type DeadBandDraft } from "./model";

/** The dead band state. */
export const deadBandPart = createPartStore(() => api.getDeadBandState());

/** The dead band draft and selection. */
export const deadBandDrafts = createTabDraftStore<DeadBandDraft>({
  tab: "deadband",
  empty: EMPTY_DEAD_BAND,
  isDirty: isDeadBandDirty,
  discardedText: (dict, profile, reconnect) =>
    reconnect ? dict.actuation.deadband.discardedReconnect : dict.actuation.deadband.discarded({ n: profile }),
});
