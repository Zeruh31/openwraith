// The write flow of every Keys tab: prepare (plans against the last full
// read, no keyboard traffic) → review (KeymapReview, with "Check that the
// keyboard kept it" as an option of the intent) → Write (armed after 500 ms;
// Enter never writes) → progress → result (EditReportView: re-sent rows,
// emptied macros and keys left on them, the persistence check, Undo).
//
// Refusals that ask for a decision are answered inside the flow:
// - replacing a critical Fn built-in (Fn + 1..4, Fn + Space) asks for an
//   explicit ConfirmCheckbox, which becomes the library's critical
//   acknowledgement; non-critical built-ins are acknowledged by the page's
//   warning;
// - a slot whose current content can't be reproduced exactly asks whether to
//   continue without Undo for it (`allow_not_undoable`).
// Each answer prepares a new plan, so Write always confirms the plan shown.

import { useEffect, useRef, useState } from "react";

import { Checkbox } from "../../../components/Controls";
import { ConfirmCheckbox } from "../../../components/ConfirmCheckbox";
import { Callout } from "../../../components/Feedback";
import { TechnicalDetails } from "../../../components/Layout";
import { keymapProblemText } from "../../../flow/labels";
import { EditReportView } from "../../../flow/Report";
import { WriteFlow, type OnProgress, type PrepareErrorView } from "../../../flow/WriteFlow";
import { useDict } from "../../../i18n";
import type {
  AppError,
  BuiltinAckIntent,
  EditReportView as EditReport,
  KeymapChangeIntent,
  KeymapEditPreview,
  KeysState,
  PlanToken,
} from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { criticalBuiltinCallout, KeymapReview, profileOfBuiltin, reviewWhat } from "./KeymapReview";
import { buildKeymapIntent } from "./intents";
import { keysFlowCount } from "./keysState";

/** What the flow learned from the user (to keep in the page's draft). */
export interface FlowAnswers {
  /** Critical built-ins the user confirmed. */
  acks: BuiltinAckIntent[];
  allowNotUndoable: boolean;
  checkPersistence: boolean;
}

/** Props of {@link KeymapEditFlow}. */
export interface KeymapEditFlowProps {
  /** The keys state the changes were made against (`based_on` = its revision). */
  state: KeysState;
  changes: readonly KeymapChangeIntent[];
  /** Critical built-ins the page already confirmed (with its own ConfirmCheckbox). */
  acks?: readonly BuiltinAckIntent[] | undefined;
  keepOrphans?: boolean | undefined;
  /** Library names by content hash (names macro slots in the review). */
  libraryNames?: ReadonlyMap<string, string> | undefined;
  /** Called with every write's report, also when the flow went away meanwhile (spend the draft here). */
  onApplied?: ((report: EditReport) => void) | undefined;
  /** Called once when the flow closes (with the report if the write ran), and with the answers given. */
  onClose: (report: EditReport | null, answers: FlowAnswers) => void;
}

type Problem = Extract<NonNullable<AppError["detail"]>, { detail: "keymap" }>["problem"];

/** Whether the label of the key's Fn built-in is confirmed on a W75 (from the keys state). */
function labelConfirmed(state: KeysState, index: number): boolean {
  return state.keys.find((key) => key.key.index === index)?.builtin?.label_confirmed ?? false;
}

function keymapProblem(error: AppError): Problem | null {
  return error.detail?.detail === "keymap" ? error.detail.problem : null;
}

/** The confirmation flow of a key-assignment change. Mount it to open it. */
export function KeymapEditFlow({
  state,
  changes,
  acks = [],
  keepOrphans = false,
  libraryNames,
  onApplied,
  onClose,
}: KeymapEditFlowProps) {
  // Counted while mounted: the page keeps the tab (and so this flow) when the keys state goes away.
  useEffect(() => {
    keysFlowCount.set((n) => n + 1);
    return () => {
      keysFlowCount.set((n) => Math.max(0, n - 1));
    };
  }, []);
  const dict = useDict();
  const t = dict.keys.review;
  const [confirmed, setConfirmed] = useState<BuiltinAckIntent[]>(() => acks.filter((ack) => ack.critical));
  const [allowNotUndoable, setAllowNotUndoable] = useState(false);
  const [checkPersistence, setCheckPersistence] = useState(false);
  // The answers as the next prepare uses them (updated after each render,
  // and at once by the persistence option, which prepares right away).
  const answers = useRef<FlowAnswers>({ acks: confirmed, allowNotUndoable, checkPersistence });
  useEffect(() => {
    answers.current = { acks: confirmed, allowNotUndoable, checkPersistence };
  });

  const prepare = async (): Promise<KeymapEditPreview> => {
    const intent = buildKeymapIntent(changes, state, {
      acks: answers.current.acks,
      keepOrphans,
      allowNotUndoable: answers.current.allowNotUndoable,
      checkPersistence: answers.current.checkPersistence,
    });
    if (intent === null) {
      const error: AppError = {
        kind: "invalid_request",
        detail: { detail: "keymap", problem: { problem: "no_changes" } },
        message: "nothing to prepare: no key-assignment change",
      };
      throw error;
    }
    return api.prepareKeymapEdit(intent);
  };

  const apply = (token: PlanToken, onProgress: OnProgress) =>
    api.applyKeymapEdit(token, onProgress).then((report) => {
      onApplied?.(report);
      return report;
    });

  const prepareError = (error: AppError): PrepareErrorView | null => {
    const problem = keymapProblem(error);
    if (problem === null) return null;
    const text = keymapProblemText(problem, dict);
    if (problem.problem === "builtin_not_acknowledged") {
      const k = profileOfBuiltin(problem.key.name);
      const label =
        k === null ? dict.keys.builtinConfirmReset({ n: state.profile }) : dict.keys.builtinConfirmProfile({ k, n: state.profile });
      const ticked = confirmed.some((ack) => ack.key === problem.key.index);
      // The same tone as the page's notice and the review: Fn + k on profile k is no danger.
      const tone = problem.critical
        ? criticalBuiltinCallout(problem.key.name, state.profile, dict)
        : { kind: "warning" as const, title: t.criticalTitle };
      return {
        body: (
          <Callout kind={tone.kind} title={tone.title} role="alert">
            <p>{text}</p>
            <p>
              {k === null
                ? dict.keys.builtinReset
                : dict.keys.builtinProfile({ k, n: state.profile, likely: !labelConfirmed(state, problem.key.index) })}
            </p>
            <ConfirmCheckbox
              checked={ticked}
              tone={tone.kind}
              label={label}
              onChange={(checked) => {
                setConfirmed((current) =>
                  checked
                    ? [...current.filter((ack) => ack.key !== problem.key.index), { key: problem.key.index, critical: true }]
                    : current.filter((ack) => ack.key !== problem.key.index),
                );
              }}
            />
            <TechnicalDetails>{error.message}</TechnicalDetails>
          </Callout>
        ),
        retry: { label: t.reviewAgain, disabledReason: ticked ? null : t.criticalNeeded },
      };
    }
    if (
      problem.problem === "unsupported_current_table" &&
      (problem.table.table === "macro" || problem.table.table === "advanced_key")
    ) {
      return {
        body: (
          <Callout kind="warning" title={t.notUndoableTitle} role="alert">
            <p>{text}</p>
            <ConfirmCheckbox
              checked={allowNotUndoable}
              tone="warning"
              label={t.notUndoableConfirm}
              onChange={setAllowNotUndoable}
            />
            <TechnicalDetails>{error.message}</TechnicalDetails>
          </Callout>
        ),
        retry: { label: t.reviewAgain, disabledReason: allowNotUndoable ? null : t.notUndoableNeeded },
      };
    }
    return null;
  };

  return (
    <WriteFlow<KeymapEditPreview, EditReport>
      kind="keymap_edit"
      prepare={prepare}
      token={(preview) => preview.token}
      title={(preview) => (preview.nothing_to_do ? t.nothingTitle : t.title({ n: preview.profile, what: reviewWhat(preview) }))}
      review={(preview) => <KeymapReview preview={preview} state={state} libraryNames={libraryNames} />}
      nothingToDo={() => <Callout kind="info" title={t.nothing} />}
      confirmLabel={() => dict.common.actions.writeToKeyboard}
      writesSettings
      prepareError={prepareError}
      reviewOptions={(preview, reprepare) =>
        preview.persistence.forced ? (
          <p className="ow-small">{t.persistence.forced}</p>
        ) : (
          <Checkbox
            checked={preview.persistence.requested}
            label={t.persistence.check({ m: preview.persistence.away_profile })}
            onChange={(checked) => {
              answers.current = { ...answers.current, checkPersistence: checked };
              setCheckPersistence(checked);
              reprepare();
            }}
          />
        )
      }
      apply={apply}
      result={(report) => <EditReportView report={report} />}
      onClose={(report) => {
        onClose(report, answers.current);
      }}
    />
  );
}
