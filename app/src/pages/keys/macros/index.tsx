// Keys > Macros: the keyboard's 10 macro slots of the active profile ("On the
// keyboard (profile N)", with the keys that run them and the macros no key
// runs), the macro library of this PC, and the macro editor (simple and
// detailed view of the steps, the recorder, the four play modes and the live
// checks). Every change goes into the tab's draft; "Review and write…" sends
// the whole draft as one key-assignment edit through the shared
// KeymapEditFlow (prepare → review with every write and its reasons, such as
// macros re-sent after Macro 1 → Write → result with Undo). After a write,
// "Try it" offers a text box to press the macro's key into.

import { useEffect, useMemo, useState } from "react";

import { ApplyBar } from "../../../components/ApplyBar";
import { EmptyState } from "../../../components/Feedback";
import { Card } from "../../../components/Layout";
import { useDict } from "../../../i18n";
import type { EditReportView, LibraryMacroView, MacroProgramView } from "../../../ipc/bindings";
import { toAppError } from "../../../ipc/errors";
import { isLocked } from "../../../state/operation";
import { toast } from "../../../state/toasts";
import {
  deleteLibraryMacro,
  KeymapEditFlow,
  libraryNames,
  loadMacroLibrary,
  macroName,
  renameLibraryMacro,
  saveLibraryMacro,
  useMacroLibrary,
  type FlowAnswers,
  type KeysTabProps,
  type LabelContext,
} from "../shared";
import { AssignDialog } from "./AssignDialog";
import {
  closeTryIt,
  discardDraft,
  ownerOf,
  setDraft,
  setFlowOpen,
  setTarget,
  spendDraft,
  updateMacros,
  useMacrosState,
} from "./draftStore";
import { DeleteLibraryDialog, Library, NameDialog } from "./Library";
import { MacroEditor } from "./MacroEditor";
import {
  addNew,
  canTry,
  chosenBindings,
  draftChanges,
  draftCount,
  draftProblems,
  EMPTY_DRAFT,
  EMPTY_PROGRAM,
  libraryMatch,
  limitsOf,
  newOf,
  removeNew,
  revertSlot,
  setAck,
  setSlotEdit,
  tryKeys,
  type MacrosDraft,
  type Target,
  type TryKey,
} from "./model";
import { newName, SlotList } from "./SlotList";
import { TryIt } from "./TryIt";
import "./macros.css";

/** What the write flow was opened with (the draft at that moment). */
interface FlowSnapshot {
  state: KeysTabProps["state"];
  changes: ReturnType<typeof draftChanges>;
  acks: MacrosDraft["acks"];
  tryKeys: TryKey[];
}

/** A library name being given. */
type Naming = { mode: "save"; program: MacroProgramView } | { mode: "rename"; entry: LibraryMacroView };

function targetKey(target: Target): string {
  return target.kind === "slot" ? `slot-${target.slot}` : `new-${target.id}`;
}

/** The macros tab. */
export default function MacrosTab({ state, blockReason }: KeysTabProps) {
  const dict = useDict();
  const t = dict.keysMacros;
  const stored = useMacrosState();
  const libraryView = useMacroLibrary();
  const owner = ownerOf(state);
  const draft = stored.owner === null || stored.owner === owner ? stored.draft : EMPTY_DRAFT;
  const names = useMemo(() => libraryNames(libraryView.library), [libraryView.library]);
  const ctx: LabelContext = { state, libraryNames: names };
  const limits = limitsOf(state);
  const editable = state.editable;
  const library = libraryView.library?.entries ?? [];
  const [flow, setFlow] = useState<FlowSnapshot | null>(null);
  const [assigning, setAssigning] = useState<number | null>(null);
  const [naming, setNaming] = useState<Naming | null>(null);
  const [deleting, setDeleting] = useState<LibraryMacroView | null>(null);
  const [focusEditor, setFocusEditor] = useState(false);

  // A flow that goes away with the tab no longer explains profile changes.
  useEffect(
    () => () => {
      setFlowOpen(false);
    },
    [],
  );

  let target = stored.target;
  if (target?.kind === "new" && newOf(draft, target.id) === undefined) target = null;

  const update = (next: MacrosDraft) => {
    setDraft(owner, next);
  };
  const open = (next: Target) => {
    setTarget(next);
    setFocusEditor(true);
  };
  const startNew = (program: MacroProgramView) => {
    updateMacros((current) => {
      const id = current.nextId;
      const base = current.owner === null || current.owner === owner ? current.draft : EMPTY_DRAFT;
      return { ...current, owner, draft: addNew(base, id, program), nextId: id + 1, target: { kind: "new", id } };
    });
    setFocusEditor(true);
  };
  const nameOf = (item: Target): string =>
    item.kind === "slot" ? macroName(item.slot, ctx, dict) : newName(draft, item.id, dict);

  const changes = draftChanges(draft, state);
  const problems = draftProblems(draft, limits);
  const count = draftCount(draft);
  const firstProblem = problems[0];
  const reason =
    blockReason ??
    (firstProblem === undefined ? null : t.checks.blocked({ name: nameOf(firstProblem.target) })) ??
    (changes.length === 0 ? t.apply.nothing : null);

  const openFlow = () => {
    if (flow !== null || isLocked() || reason !== null) return;
    setFlowOpen(true);
    setFlow({ state, changes, acks: draft.acks, tryKeys: tryKeys(draft, state) });
  };

  const onApplied = (report: EditReportView, snapshot: FlowSnapshot) => {
    if (report.anything_written) spendDraft(canTry(report) ? snapshot.tryKeys : null);
  };

  const onFlowClosed = (report: EditReportView | null, answers: FlowAnswers) => {
    setFlow(null);
    setFlowOpen(false);
    if (report?.anything_written === true || answers.acks.length === 0) return;
    updateMacros((current) => {
      let next = current.draft;
      for (const ack of answers.acks) next = setAck(next, ack.key, true);
      return { ...current, draft: next };
    });
  };

  const saveTo = (program: MacroProgramView) => {
    setNaming({ mode: "save", program });
  };

  const pending = naming;
  let namingDialog = null;
  if (pending !== null) {
    const existing = pending.mode === "save" ? libraryMatch(pending.program, library) : pending.entry;
    namingDialog =
      pending.mode === "save" ? (
        <NameDialog
          title={t.library.saveTitle}
          initial={existing?.name ?? ""}
          note={existing === undefined ? null : t.library.alreadySaved({ name: existing.name })}
          confirmLabel={t.library.save}
          onSubmit={(name) =>
            saveLibraryMacro(name, pending.program, dict).then(() => {
              toast({ kind: "success", title: t.library.saved({ name }) });
            })
          }
          onClose={() => {
            setNaming(null);
          }}
        />
      ) : (
        <NameDialog
          title={t.library.renameTitle({ name: pending.entry.name })}
          initial={pending.entry.name}
          confirmLabel={t.library.rename}
          onSubmit={(name) =>
            renameLibraryMacro(pending.entry.content_hash, name, dict).then(() => {
              toast({ kind: "success", title: t.library.renamed({ name }) });
            })
          }
          onClose={() => {
            setNaming(null);
          }}
        />
      );
  }

  const assignSlot = assigning;

  return (
    <div className="keys-macros">
      <p className="ow-muted keys-macros__intro">{t.intro}</p>
      {stored.tryIt === null ? null : <TryIt keys={stored.tryIt} onDone={closeTryIt} />}
      <div className="keys-macros__layout">
        <div className="keys-macros__slots-pane">
          <SlotList
            state={state}
            ctx={ctx}
            draft={draft}
            target={target}
            library={library}
            editable={editable}
            onOpen={open}
            onNew={() => {
              startNew(EMPTY_PROGRAM);
            }}
            onAssign={setAssigning}
            onRemove={(slot) => {
              update(setSlotEdit(draft, state, slot, { remove: true }));
            }}
            onKeep={(slot) => {
              update(setSlotEdit(draft, state, slot, { remove: false }));
            }}
            onRevert={(item) => {
              if (item.kind === "slot") {
                update(revertSlot(draft, item.slot));
              } else {
                update(removeNew(draft, item.id));
              }
            }}
            onSave={saveTo}
          />
        </div>
        <div className="keys-macros__editor-pane">
          {target === null ? (
            <Card className="keys-macros__editor-empty">
              <EmptyState icon="keys" title={t.editor.emptyTitle}>
                <p>{t.editor.emptyBody}</p>
              </EmptyState>
            </Card>
          ) : (
            <MacroEditor
              key={targetKey(target)}
              state={state}
              ctx={ctx}
              target={target}
              name={nameOf(target)}
              draft={draft}
              onDraft={update}
              limits={limits}
              view={stored.view}
              onView={(view) => {
                updateMacros((current) => ({ ...current, view }));
              }}
              delayMode={stored.delayMode}
              onDelayMode={(delayMode) => {
                updateMacros((current) => ({ ...current, delayMode }));
              }}
              fixedMs={stored.fixedMs}
              onFixedMs={(fixedMs) => {
                updateMacros((current) => ({ ...current, fixedMs }));
              }}
              library={library}
              onSaveToLibrary={saveTo}
              onRevert={() => {
                if (target.kind === "slot") {
                  update(revertSlot(draft, target.slot));
                } else {
                  update(removeNew(draft, target.id));
                  setTarget(null);
                }
              }}
              onKeep={() => {
                if (target.kind === "slot") update(setSlotEdit(draft, state, target.slot, { remove: false }));
              }}
              onClose={() => {
                setTarget(null);
              }}
              editable={editable}
              autoFocus={focusEditor}
            />
          )}
        </div>
        <div className="keys-macros__library-pane">
          <Library
            state={state}
            view={libraryView}
            editable={editable}
            onPutOnKey={startNew}
            onRename={(entry) => {
              setNaming({ mode: "rename", entry });
            }}
            onDelete={setDeleting}
            onRetry={() => {
              loadMacroLibrary(dict, true);
            }}
          />
        </div>
      </div>
      <ApplyBar
        summary={count === 0 ? null : t.apply.summary({ n: count })}
        dirty={count > 0}
        onDiscard={discardDraft}
        onApply={openFlow}
        disabledReason={reason}
      />
      {assignSlot === null ? null : (
        <AssignDialog
          state={state}
          ctx={ctx}
          name={macroName(assignSlot, ctx, dict)}
          chosen={chosenBindings(draft, { kind: "slot", slot: assignSlot })}
          acks={draft.acks}
          onAck={(key, confirmed) => {
            update(setAck(draft, key, confirmed));
          }}
          onAssign={(bind) => {
            update(setSlotEdit(draft, state, assignSlot, { bind }));
            setAssigning(null);
          }}
          onClose={() => {
            setAssigning(null);
          }}
        />
      )}
      {namingDialog}
      {deleting === null ? null : (
        <DeleteLibraryDialog
          entry={deleting}
          onCancel={() => {
            setDeleting(null);
          }}
          onConfirm={() => {
            const entry = deleting;
            setDeleting(null);
            deleteLibraryMacro(entry.content_hash, dict).then(
              () => {
                toast({ kind: "success", title: t.library.deleted({ name: entry.name }) });
              },
              (failure: unknown) => {
                const error = toAppError(failure);
                toast({
                  kind: "danger",
                  title: dict.errors.kind[error.kind].title,
                  body: dict.errors.kind[error.kind].body,
                  details: error.message,
                });
              },
            );
          }}
        />
      )}
      {flow === null ? null : (
        <KeymapEditFlow
          state={flow.state}
          changes={flow.changes}
          acks={flow.acks}
          libraryNames={names}
          onApplied={(report) => {
            onApplied(report, flow);
          }}
          onClose={onFlowClosed}
        />
      )}
    </div>
  );
}
