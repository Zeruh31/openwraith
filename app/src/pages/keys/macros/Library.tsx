// "Library (this PC)": macros saved under a name on this computer (a small
// file in the app's data folder; nothing is sent to the keyboard). Each one
// says where it is on the keyboard (matched by the library's content hash),
// and can be put on a key (a new macro of the draft), renamed or deleted.
// Also the dialog that names a macro when it is saved or renamed.

import { useId, useState } from "react";

import { Button } from "../../../components/Button";
import { ConfirmDialog, Dialog } from "../../../components/Dialog";
import { Spinner } from "../../../components/Feedback";
import { Card } from "../../../components/Layout";
import { ErrorView } from "../../../flow/ErrorView";
import { useDict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type { AppError, KeysState, LibraryMacroView, MacroLibraryView, MacroProgramView } from "../../../ipc/bindings";
import { toAppError } from "../../../ipc/errors";
import { programText } from "../shared";

/** The longest library name (the library's limit). */
export const NAME_MAX = 64;

/** The library's state as the tab gets it. */
export interface LibraryView {
  library: MacroLibraryView | null;
  loading: boolean;
  error: AppError | null;
}

/** The keyboard slots holding a library macro ("Macro 3"). */
export function slotsHolding(entry: LibraryMacroView, state: KeysState): number[] {
  return state.macros
    .filter((slot) => slot.content.state === "program" && slot.content.content_hash === entry.content_hash)
    .map((slot) => slot.slot);
}

/** Props of {@link Library}. */
export interface LibraryProps {
  state: KeysState;
  view: LibraryView;
  editable: boolean;
  onPutOnKey: (program: MacroProgramView) => void;
  onRename: (entry: LibraryMacroView) => void;
  onDelete: (entry: LibraryMacroView) => void;
  onRetry: () => void;
}

/** The macro library of this PC. */
export function Library({ state, view, editable, onPutOnKey, onRename, onDelete, onRetry }: LibraryProps) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.keysMacros.library;
  const entries = view.library?.entries ?? [];
  let body;
  if (view.library === null && view.loading) {
    body = (
      <p className="ow-row">
        <Spinner />
        <span>{t.loading}</span>
      </p>
    );
  } else if (view.library === null && view.error !== null) {
    body = (
      <>
        <ErrorView error={view.error} written="none" />
        <div>
          <Button size="sm" icon="refresh" onClick={onRetry}>
            {dict.common.actions.retry}
          </Button>
        </div>
      </>
    );
  } else if (entries.length === 0) {
    body = <p className="ow-small">{t.empty}</p>;
  } else {
    body = (
      <ul className="keys-macros__library" aria-label={t.listLabel}>
        {entries.map((entry) => {
          const slots = slotsHolding(entry, state);
          return (
            <li key={entry.content_hash} className="keys-macros__slot">
              <div className="keys-macros__slot-main">
                <span className="ow-strong">{entry.name}</span>
                <span className="ow-small ow-muted">{programText(entry.program, dict)}</span>
                {slots.length === 0 ? null : (
                  <span className="ow-small">
                    {t.onKeyboard({ slots: format.list(slots.map((slot) => dict.keys.action.macro({ n: slot + 1 }))) })}
                  </span>
                )}
              </div>
              <div className="keys-macros__slot-actions">
                <Button
                  size="sm"
                  disabled={!editable}
                  aria-label={t.actions.putOnKey({ name: entry.name })}
                  onClick={() => {
                    onPutOnKey(entry.program);
                  }}
                >
                  {t.labels.putOnKey}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={t.actions.rename({ name: entry.name })}
                  onClick={() => {
                    onRename(entry);
                  }}
                >
                  {t.labels.rename}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  icon="trash"
                  aria-label={t.actions.delete({ name: entry.name })}
                  onClick={() => {
                    onDelete(entry);
                  }}
                >
                  {t.labels.delete}
                </Button>
              </div>
            </li>
          );
        })}
      </ul>
    );
  }
  return (
    <Card title={t.title} className="keys-macros__library-card">
      <p className="ow-small ow-muted">{t.intro}</p>
      {body}
    </Card>
  );
}

/** Props of {@link NameDialog}. */
export interface NameDialogProps {
  title: string;
  initial: string;
  confirmLabel: string;
  /** A note above the field (e.g. "already saved as …"). */
  note?: string | null | undefined;
  /** Saves the name; rejects with an `AppError` to show it. */
  onSubmit: (name: string) => Promise<unknown>;
  onClose: () => void;
}

/** Asks for a library name (1-64 characters) and saves it. Mount it to open it. */
export function NameDialog({ title, initial, confirmLabel, note, onSubmit, onClose }: NameDialogProps) {
  const dict = useDict();
  const t = dict.keysMacros.library;
  const id = useId();
  const [name, setName] = useState(initial);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const trimmed = name.trim();
  const reason = trimmed === "" ? t.nameEmpty : trimmed.length > NAME_MAX ? t.nameTooLong({ max: NAME_MAX }) : null;

  const submit = () => {
    if (reason !== null || pending) return;
    setPending(true);
    setError(null);
    onSubmit(trimmed).then(
      () => {
        setPending(false);
        onClose();
      },
      (failure: unknown) => {
        setPending(false);
        setError(toAppError(failure));
      },
    );
  };

  return (
    <Dialog
      open
      title={title}
      size="sm"
      onClose={onClose}
      actions={
        <>
          <Button onClick={onClose}>{dict.common.actions.cancel}</Button>
          <Button variant="primary" disabled={pending} disabledReason={reason} onClick={submit}>
            {confirmLabel}
          </Button>
        </>
      }
    >
      <form
        className="ow-stack"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        {note === null || note === undefined ? null : <p className="ow-small">{note}</p>}
        <div className="keys-macros__field">
          <label htmlFor={`${id}-name`} className="ow-field__label">
            {t.name}
          </label>
          <input
            id={`${id}-name`}
            type="text"
            className="ow-input keys-macros__name-input"
            value={name}
            maxLength={NAME_MAX + 16}
            autoComplete="off"
            aria-describedby={`${id}-hint`}
            aria-invalid={reason !== null && name !== initial ? true : undefined}
            onChange={(event) => {
              setName(event.currentTarget.value);
            }}
          />
          <p id={`${id}-hint`} className="ow-small ow-muted">
            {t.nameHint({ max: NAME_MAX })}
          </p>
        </div>
        {error === null ? null : <ErrorView error={error} written="none" />}
      </form>
    </Dialog>
  );
}

/** Props of {@link DeleteLibraryDialog}. */
export interface DeleteLibraryDialogProps {
  entry: LibraryMacroView;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Confirms deleting a library macro (this PC only). */
export function DeleteLibraryDialog({ entry, onConfirm, onCancel }: DeleteLibraryDialogProps) {
  const dict = useDict();
  const t = dict.keysMacros.library;
  return (
    <ConfirmDialog
      open
      title={t.deleteTitle({ name: entry.name })}
      confirmLabel={t.deleteConfirm}
      danger
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <p>{t.deleteBody}</p>
    </ConfirmDialog>
  );
}
