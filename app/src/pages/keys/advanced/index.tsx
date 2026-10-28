// Keys > Advanced keys (design B4): dynamic keystrokes (DKS), tap or hold,
// toggle. The tab shows a card per used slot ("3 of 40 used"), including
// slots no key uses and kinds OpenWraith can't read ("Kept as read"); tiles
// start a new advanced key; the editor dialog edits one (the DKS grid, the
// first point, quick starts, the live sentence). Every button only changes
// the tab's draft (new, changed, assigned, deleted: a delete puts every key
// that uses the slot back to Default and empties the slot, in one change).
// "Review and write…" opens the shared keymap write flow: one Write = one
// library operation, with the plan's own reasons and Undo.

import { useEffect, useId, useState } from "react";

import { ApplyBar } from "../../../components/ApplyBar";
import { Callout } from "../../../components/Feedback";
import { useDict } from "../../../i18n";
import type { KeysState } from "../../../ipc/bindings";
import { tableName } from "../../../flow/labels";
import { KeymapEditFlow, type KeysTabProps } from "../shared";
import { AdvancedEditor, type EditorRequest, type EditorResult } from "./AdvancedEditor";
import { AssignDialog } from "./AssignDialog";
import {
  discardAdvancedDraft,
  draftChange,
  dropChange,
  effectiveDraft,
  keepAcks,
  ownerOf,
  setAdvancedFlowOpen,
  useAdvancedStore,
} from "./draftStore";
import {
  ADVANCED_TYPES,
  changesOf,
  createdItems,
  draftAcks,
  hasFreeSlot,
  sameDef,
  slotCards,
} from "./model";
import { NewCard, SlotCard } from "./SlotCard";
import "./keys-advanced.css";

/** Why advanced keys can't be edited now (the keys state's own reasons), or `null`. */
function editBlock(state: KeysState, dict: ReturnType<typeof useDict>): string | null {
  if (!state.editable) {
    const disabled = state.disabled;
    return disabled?.reason === "unreadable_table"
      ? dict.keys.disabled.unreadable_table({ table: tableName(disabled.table, dict) })
      : dict.keys.disabled.unsupported;
  }
  return state.advanced_editable ? null : dict.keys.advancedDisabled;
}

/** The Advanced keys tab. */
export default function AdvancedTab({ state, blockReason }: KeysTabProps) {
  const dict = useDict();
  const t = dict.keysAdvanced;
  const ids = useId();
  const stored = useAdvancedStore();
  const owner = ownerOf(state);
  const draft = effectiveDraft(stored, owner);
  const [editor, setEditor] = useState<{ request: EditorRequest; seq: number } | null>(null);
  const [assigning, setAssigning] = useState<number | null>(null);
  const [flowOpen, setFlowOpen] = useState(false);
  const [seq, setSeq] = useState(0);

  // The store's "discarded" toast is not shown for changes the flow makes.
  useEffect(() => {
    setAdvancedFlowOpen(flowOpen);
  }, [flowOpen]);
  useEffect(
    () => () => {
      setAdvancedFlowOpen(false);
    },
    [],
  );

  const editReason = editBlock(state, dict);
  const canEdit = editReason === null;
  const free = hasFreeSlot(state);
  const cards = slotCards(state, draft);
  const created = createdItems(draft);
  const count = draft.items.length;

  const open = (request: EditorRequest) => {
    setSeq((n) => n + 1);
    setEditor({ request, seq: seq + 1 });
  };

  const save = (request: EditorRequest, result: EditorResult) => {
    if (request.mode === "create") {
      if (result.target === null) return;
      draftChange(
        owner,
        { kind: "create", layer: result.target.layer, key: result.target.key, def: result.def },
        { replaces: request.item?.id ?? null, ack: result.ack },
      );
    } else {
      const current = state.advanced.find((slot) => slot.slot === request.slot)?.content;
      if (current?.state === "def" && sameDef(current.def, result.def)) {
        // Back to what the keyboard has: no change.
        if (request.itemId !== null) dropChange(owner, request.itemId);
      } else {
        draftChange(owner, { kind: "edit", slot: request.slot, def: result.def }, { replaces: request.itemId });
      }
    }
    setEditor(null);
  };

  const slotName = (slot: number) => {
    const content = state.advanced.find((item) => item.slot === slot)?.content;
    return content?.state === "def"
      ? dict.keys.action.advancedTyped({ n: slot + 1, type: dict.keys.advancedTypes[content.def.type] })
      : dict.keys.action.advanced({ n: slot + 1 });
  };

  return (
    <div className="keys-adv">
      <p className="ow-muted">{t.intro}</p>
      {editReason !== null && state.editable ? (
        <Callout kind="warning" role="status">
          {editReason}
        </Callout>
      ) : null}

      <section className="keys-adv-section" aria-labelledby={`${ids}-new`}>
        <h2 id={`${ids}-new`} className="keys-adv-section__title">
          {t.create.title}
        </h2>
        <p className="ow-small ow-muted">{t.create.hint}</p>
        <div className="keys-adv-tiles">
          {ADVANCED_TYPES.map((type) => (
            <button
              key={type}
              type="button"
              className="keys-adv-tile"
              aria-label={t.editor.titleNew({ type: dict.keys.advancedTypes[type] })}
              aria-describedby={`${ids}-tile-${type}`}
              disabled={!canEdit || !free}
              onClick={() => {
                open({ mode: "create", type, item: null });
              }}
            >
              <span className="keys-adv-tile__name">{dict.keys.advancedTypes[type]}</span>
              <span id={`${ids}-tile-${type}`} className="keys-adv-tile__text ow-small ow-muted">
                {t.create.sentences[type]}
              </span>
            </button>
          ))}
        </div>
        {free || !canEdit ? null : <p className="ow-small">{t.create.noFreeSlot}</p>}
      </section>

      <section className="keys-adv-section" aria-labelledby={`${ids}-list`}>
        <div className="keys-adv-section__head">
          <h2 id={`${ids}-list`} className="keys-adv-section__title">
            {t.list.title({ n: state.profile })}
          </h2>
          <span className="ow-muted ow-small ow-num">
            {dict.keys.slot.used({ n: state.advanced_used, total: state.limits.advanced_slots })}
          </span>
        </div>
        {created.length === 0 ? null : (
          <>
            <h3 className="keys-adv-section__subtitle">{t.list.newTitle}</h3>
            <ul className="keys-adv-cards">
              {created.map((item) => (
                <li key={item.id}>
                  <NewCard
                    item={item}
                    state={state}
                    canEdit={canEdit}
                    onEdit={(edited) => {
                      open({ mode: "create", type: edited.def.type, item: edited });
                    }}
                    onDrop={(id) => {
                      dropChange(owner, id);
                    }}
                  />
                </li>
              ))}
            </ul>
          </>
        )}
        {cards.length === 0 ? (
          <p className="ow-muted">{t.list.empty}</p>
        ) : (
          <ul className="keys-adv-cards">
            {cards.map((model) => (
              <li key={model.slot.slot}>
                <SlotCard
                  model={model}
                  state={state}
                  canEdit={canEdit}
                  onEdit={(slot, def, itemId) => {
                    open({ mode: "edit", slot, def, itemId });
                  }}
                  onAssign={setAssigning}
                  onDelete={(slot) => {
                    draftChange(owner, { kind: "delete", slot });
                  }}
                  onDrop={(id) => {
                    dropChange(owner, id);
                  }}
                />
              </li>
            ))}
          </ul>
        )}
      </section>

      <ApplyBar
        summary={
          count > 0 ? (
            <p className="ow-num">
              <span className="ow-strong">{t.draft.notWritten}</span>
              <span> · {t.draft.count({ n: count })}</span>
            </p>
          ) : (
            <p className="ow-muted">{t.draft.idle}</p>
          )
        }
        dirty={count > 0}
        onDiscard={discardAdvancedDraft}
        onApply={() => {
          if (blockReason === null && count > 0) setFlowOpen(true);
        }}
        disabledReason={blockReason}
      />

      {editor === null ? null : (
        <AdvancedEditor
          key={editor.seq}
          state={state}
          request={editor.request}
          confirmedKeys={draft.acks.map((ack) => ack.key)}
          onSave={(result) => {
            save(editor.request, result);
          }}
          onClose={() => {
            setEditor(null);
          }}
        />
      )}
      {assigning === null ? null : (
        <AssignDialog
          state={state}
          name={slotName(assigning)}
          onSave={(target, ack) => {
            draftChange(owner, { kind: "assign", slot: assigning, layer: target.layer, key: target.key }, { ack });
            setAssigning(null);
          }}
          onClose={() => {
            setAssigning(null);
          }}
        />
      )}
      {flowOpen ? (
        <KeymapEditFlow
          state={state}
          changes={changesOf(draft)}
          acks={draftAcks(draft, state)}
          onApplied={(report) => {
            // Everything planned was written: the draft is spent (also when the flow went away meanwhile).
            if (report.outcome === "completed") discardAdvancedDraft();
          }}
          onClose={(_report, answers) => {
            keepAcks(owner, answers.acks);
            setFlowOpen(false);
          }}
        />
      ) : null}
    </div>
  );
}
