// Keys > Assignments: keys that refer to an empty macro or advanced-key slot
// (or hold a reference OpenWraith can't read). Each one can be reset to its
// default with one click: that only adds the reset to the draft; it is
// written with the other changes after the review.

import { Button } from "../../../components/Button";
import { Card, TechnicalDetails } from "../../../components/Layout";
import { keyRefText, tableName } from "../../../flow/labels";
import { useDict, type Dict } from "../../../i18n";
import type { BrokenReferenceView } from "../../../ipc/bindings";
import { isPending, type AssignDraft } from "./model";

/** What is wrong with a key's reference, in words. */
export function brokenText(item: BrokenReferenceView, dict: Dict): string {
  const key = keyRefText(item.key, item.layer, dict);
  return item.problem.kind === "empty_slot"
    ? dict.keysAssign.broken.emptySlot({ key, slot: tableName(item.problem.slot, dict) })
    : dict.keysAssign.broken.malformed({ key });
}

/** Props of {@link BrokenReferences}. */
export interface BrokenReferencesProps {
  items: readonly BrokenReferenceView[];
  draft: AssignDraft;
  /** Why the draft can't be changed now, or `null`. */
  editBlock: string | null;
  onReset: (item: BrokenReferenceView) => void;
}

/** The list of keys with broken references, each with "Reset key". */
export function BrokenReferences({ items, draft, editBlock, onReset }: BrokenReferencesProps) {
  const dict = useDict();
  const t = dict.keysAssign.broken;
  if (items.length === 0) return null;
  const technical = items
    .map(
      (item) =>
        `${keyRefText(item.key, item.layer, dict)}: ${item.hex}${item.problem.kind === "malformed" ? ` (${item.problem.reason})` : ""}`,
    )
    .join("\n");
  return (
    <Card title={t.title} className="keys-assign-broken">
      <div className="ow-stack">
        <p className="ow-muted">{t.intro}</p>
        <ul className="keys-assign-broken__list">
          {items.map((item) => {
            const pending = isPending(draft, item.layer, item.key.index);
            const key = keyRefText(item.key, item.layer, dict);
            return (
              <li key={`${item.layer}-${item.key.index}`} className="keys-assign-broken__item">
                <span>{brokenText(item, dict)}</span>
                {pending ? (
                  <span className="ow-small ow-muted">{t.inDraft}</span>
                ) : item.resettable ? (
                  <Button
                    size="sm"
                    aria-label={t.resetLabel({ key })}
                    disabledReason={editBlock}
                    onClick={() => {
                      onReset(item);
                    }}
                  >
                    {t.reset}
                  </Button>
                ) : (
                  <span className="ow-small ow-muted">{t.notResettable}</span>
                )}
              </li>
            );
          })}
        </ul>
        <TechnicalDetails>{technical}</TechnicalDetails>
      </div>
    </Card>
  );
}
