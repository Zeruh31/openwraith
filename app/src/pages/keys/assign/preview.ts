// Browser-preview UI state of Keys > Assignments (`previewUi("keysAssign")`):
// the layer, a selected key, a draft and whether the review opens by itself.
// Outside the preview `previewUi` returns `undefined` and the tab starts
// from its stored state.

import type { LayerView } from "../../../ipc/bindings";
import { EMPTY_DRAFT, type AssignDraft, type DraftTarget, type SwapPair } from "./model";

/** What a preview scenario can preset on the tab. */
export interface KeysAssignPreviewUi {
  layer?: LayerView | undefined;
  /** A key index to select. */
  key?: number | undefined;
  /** Drafted targets (e.g. the recorded plan's changes). */
  targets?: readonly DraftTarget[] | undefined;
  /** Drafted swap pairs (both keys also need their targets). */
  swaps?: readonly SwapPair[] | undefined;
  /** Fn-layer keys whose essential built-in the user agreed to replace. */
  confirmed?: readonly number[] | undefined;
  /** Open the review right away (to show the recorded prepare and apply). */
  open?: boolean | undefined;
}

/** The tab's starting state from the preview UI state. */
export interface AssignSeed {
  layer: LayerView | undefined;
  selected: number | undefined;
  draft: AssignDraft | undefined;
  open: boolean;
}

/** The starting state from the preview UI state (nothing outside the preview). */
export function seedFrom(ui: KeysAssignPreviewUi | undefined): AssignSeed {
  const hasDraft = (ui?.targets?.length ?? 0) > 0;
  return {
    layer: ui?.layer,
    selected: ui?.key,
    draft: hasDraft
      ? {
          ...EMPTY_DRAFT,
          targets: [...(ui?.targets ?? [])],
          swaps: [...(ui?.swaps ?? [])],
          confirmed: [...(ui?.confirmed ?? [])],
        }
      : undefined,
    open: ui?.open === true,
  };
}
