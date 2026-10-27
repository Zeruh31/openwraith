// Browser-preview UI state of the actuation page (`previewUi("actuation")`):
// preselected keys, a draft, the "Show on keys" mode and whether the review
// opens by itself. Outside the preview `previewUi` returns `undefined` and
// the page starts empty.

import type { RtIntent } from "../../ipc/bindings";
import { EMPTY_DRAFT, type ShowMode, type TriggerDraft } from "./model";

/** What a preview scenario can preset on the actuation page. */
export interface ActuationPreviewUi {
  /** Key indices to select. */
  keys?: readonly number[] | undefined;
  /** A drafted actuation point, in trigger units. */
  actuation?: number | undefined;
  /** A drafted Rapid Trigger setting. */
  rt?: RtIntent | undefined;
  show?: ShowMode | undefined;
  /** Open the review right away (to show the recorded prepare and apply). */
  open?: boolean | undefined;
}

/** The page's starting state. */
export interface PageSeed {
  selection: Set<number>;
  draft: TriggerDraft;
  show: ShowMode;
  open: boolean;
}

/** The starting state from the preview UI state (or an empty page). */
export function seedFrom(ui: ActuationPreviewUi | undefined): PageSeed {
  const draft: TriggerDraft = { ...EMPTY_DRAFT, actuation: ui?.actuation ?? null };
  const rt = ui?.rt;
  if (rt !== undefined) {
    switch (rt.mode) {
      case "off":
        draft.rt = "off";
        break;
      case "same":
        draft.rt = "same";
        draft.sensitivity = rt.sensitivity;
        break;
      case "separate":
        draft.rt = "separate";
        draft.press = rt.press;
        draft.release = rt.release;
        break;
    }
  }
  return {
    selection: new Set(ui?.keys ?? []),
    draft,
    show: ui?.show ?? "actuation",
    open: ui?.open === true,
  };
}
