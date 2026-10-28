// "Add a game": choose the program (a running one, Browse… with the Rust
// file dialog, "detect the next program I open", or a typed file name),
// name it and choose its profile. The program travels as a token (the path
// stays in Rust). Adding the rule is the user's consent for it.

import { useEffect, useId, useRef, useState } from "react";

import { Button } from "../../../components/Button";
import { Countdown } from "../../../components/Countdown";
import { RadioGroup } from "../../../components/Controls";
import { Dialog } from "../../../components/Dialog";
import { Callout, Spinner } from "../../../components/Feedback";
import { ErrorView } from "../../../flow/ErrorView";
import { useDict } from "../../../i18n";
import type { AppError, ProgramView } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { PROFILES, profileLabel, useProfileNames } from "../../../state/profileNames";
import { exeNameProblem, LABEL_MAX, labelFromExe, typedProgramName, type NewRule } from "./model";

/** How long `detect_next_program` waits. */
export const DETECT_MS = 30_000;

/** The chosen program: one with a token, or a typed file name. */
type Chosen = { kind: "picked"; program: ProgramView } | { kind: "name"; exe: string };

/** Props of {@link AddGameDialog}. */
export interface AddGameDialogProps {
  /** This build can list running programs and see the program in front. */
  programsAvailable: boolean;
  /** The profile offered first. */
  initialProfile: number;
  /** Saves the rule; resolves with the error when it was refused (`null`: added, the dialog closes). */
  onAdd: (rule: NewRule) => Promise<AppError | null>;
  onClose: () => void;
}

/** The "Add a game" dialog (mount it to show it). */
export function AddGameDialog({ programsAvailable, initialProfile, onAdd, onClose }: AddGameDialogProps) {
  const dict = useDict();
  const t = dict.settings.games.add;
  const names = useProfileNames();
  const ids = {
    name: useId(),
    nameHint: useId(),
    profile: useId(),
    typed: useId(),
    typedHint: useId(),
    unavailable: useId(),
  };
  const alive = useRef(true);
  const [chosen, setChosen] = useState<Chosen | null>(null);
  const [running, setRunning] = useState<ProgramView[] | null>(null);
  const [runningBusy, setRunningBusy] = useState(false);
  const [detecting, setDetecting] = useState(0);
  const [detectNone, setDetectNone] = useState(false);
  const [typed, setTyped] = useState("");
  const [typedTouched, setTypedTouched] = useState(false);
  const [label, setLabel] = useState("");
  const [labelEdited, setLabelEdited] = useState(false);
  const [profile, setProfile] = useState(initialProfile);
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  /** The detection whose answer is wanted (a stopped one is ignored). */
  const detection = useRef(0);
  // Keyboard focus stays in the dialog when a button it was on is disabled
  // or removed (Detect, Stop waiting, the running-list button).
  const detectRef = useRef<HTMLButtonElement>(null);
  const stopRef = useRef<HTMLButtonElement>(null);
  const runningRef = useRef<HTMLButtonElement>(null);
  const wasDetecting = useRef(false);
  const wasRunningBusy = useRef(false);
  useEffect(() => {
    const lost = () => {
      const active = document.activeElement;
      return active === null || active === document.body;
    };
    if (detecting !== 0 && !wasDetecting.current) {
      if (lost()) stopRef.current?.focus();
    } else if (detecting === 0 && wasDetecting.current) {
      if (lost()) detectRef.current?.focus();
    }
    wasDetecting.current = detecting !== 0;
  }, [detecting]);
  useEffect(() => {
    if (!runningBusy && wasRunningBusy.current) {
      const active = document.activeElement;
      if (active === null || active === document.body) runningRef.current?.focus();
    }
    wasRunningBusy.current = runningBusy;
  }, [runningBusy]);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const choose = (next: Chosen) => {
    setChosen(next);
    setError(null);
    if (!labelEdited) {
      setLabel(labelFromExe(next.kind === "picked" ? next.program.exe : next.exe));
    }
  };

  const loadRunning = () => {
    setRunningBusy(true);
    api
      .listRunningPrograms()
      .then((programs) => {
        if (alive.current) setRunning(programs);
      })
      .catch((cause: unknown) => {
        if (alive.current) setError(toAppError(cause));
      })
      .finally(() => {
        if (alive.current) setRunningBusy(false);
      });
  };

  const browse = () => {
    api
      .pickGameProgram({ title: t.browseTitle, filter: t.browseFilter })
      .then((program) => {
        if (alive.current && program !== null) choose({ kind: "picked", program });
      })
      .catch((cause: unknown) => {
        if (alive.current) setError(toAppError(cause));
      });
  };

  const detect = () => {
    detection.current += 1;
    const mine = detection.current;
    setDetecting(mine);
    setDetectNone(false);
    api
      .detectNextProgram()
      .then((program) => {
        if (!alive.current || detection.current !== mine) return;
        if (program === null) {
          setDetectNone(true);
        } else {
          choose({ kind: "picked", program });
        }
      })
      .catch((cause: unknown) => {
        if (alive.current && detection.current === mine) setError(toAppError(cause));
      })
      .finally(() => {
        if (alive.current && detection.current === mine) setDetecting(0);
      });
  };

  const stopDetecting = () => {
    // The backend's wait ends by itself; its answer is ignored.
    detection.current += 1;
    setDetecting(0);
  };

  const typedProblem = exeNameProblem(typed);
  const applyTyped = () => {
    setTypedTouched(true);
    if (typedProblem === null) choose({ kind: "name", exe: typedProgramName(typed) });
  };

  /** The program: the one chosen, else a valid typed name not applied yet. */
  const effective: Chosen | null =
    chosen ?? (typedProblem === null ? { kind: "name", exe: typedProgramName(typed) } : null);
  const missing =
    effective === null
      ? typedProblem === "folder"
        ? t.typeNameInvalid
        : t.needProgram
      : label.trim() === ""
        ? t.needName
        : null;
  /** Without program access the running list and detection are off; one note says why. */
  const unavailable = programsAvailable ? undefined : ids.unavailable;

  const add = () => {
    if (effective === null || missing !== null || adding) return;
    setAdding(true);
    setError(null);
    const program: NewRule["program"] =
      effective.kind === "picked"
        ? { source: "picked", token: effective.program.token }
        : { source: "name", exe: effective.exe };
    void onAdd({ label: label.trim(), program, profile }).then((refused) => {
      if (!alive.current) return;
      setAdding(false);
      if (refused !== null) setError(refused);
    });
  };

  const picked = chosen?.kind === "picked" ? chosen.program : null;
  const exe = chosen === null ? null : chosen.kind === "picked" ? chosen.program.exe : chosen.exe;

  return (
    <Dialog
      open
      title={t.title}
      onClose={onClose}
      size="md"
      actions={
        <>
          <Button onClick={onClose}>{dict.common.actions.cancel}</Button>
          <Button variant="primary" icon="plus" disabledReason={missing} disabled={adding} onClick={add}>
            {t.confirm}
          </Button>
        </>
      }
    >
      <div className="ow-stack st-add">
        <section className="st-add__section" aria-labelledby={`${ids.name}-program`}>
          <h3 id={`${ids.name}-program`} className="st-add__heading">
            {t.program}
          </h3>
          {exe === null ? (
            <p className="ow-muted">{t.none}</p>
          ) : (
            <div className="st-add__chosen" role="status">
              <p className="ow-mono ow-strong ow-break">{exe}</p>
              {chosen?.kind === "picked" ? (
                <p className="ow-muted ow-small ow-break">
                  {chosen.program.folder === null ? t.folderUnknown : t.folder({ folder: chosen.program.folder })}
                </p>
              ) : (
                <p className="ow-muted ow-small">{t.typeNameHint}</p>
              )}
            </div>
          )}
          {picked?.generic_name === true ? (
            <Callout kind="warning" title={dict.settings.games.rules.genericShort}>
              <p>{dict.settings.games.rules.generic}</p>
            </Callout>
          ) : null}
          <div className="st-add__ways">
            <Button
              ref={runningRef}
              size="sm"
              icon="search"
              disabled={!programsAvailable || runningBusy}
              aria-describedby={unavailable}
              onClick={loadRunning}
            >
              {running === null ? t.runningShow : t.refresh}
            </Button>
            <Button size="sm" icon="folder" onClick={browse}>
              {t.browse}
            </Button>
            <Button
              ref={detectRef}
              size="sm"
              icon="pointer"
              disabled={!programsAvailable || detecting !== 0}
              aria-describedby={unavailable}
              onClick={detect}
            >
              {t.detect}
            </Button>
          </div>
          {programsAvailable ? null : (
            <p id={ids.unavailable} className="ow-muted ow-small">
              {t.unavailable}
            </p>
          )}
          {detecting === 0 ? null : (
            <Callout
              kind="info"
              actions={
                <Button ref={stopRef} size="sm" onClick={stopDetecting}>
                  {t.detectStop}
                </Button>
              }
            >
              <p className="ow-row">
                <Spinner />
                <span>{t.detecting}</span>
              </p>
              <p className="ow-muted ow-small">
                <Countdown key={detecting} remainingMs={DETECT_MS} render={(time) => t.detectLeft({ time })} />
              </p>
            </Callout>
          )}
          {detectNone ? <p className="ow-muted">{t.detectNone}</p> : null}
          {running === null ? null : running.length === 0 ? (
            <p className="ow-muted">{t.runningEmpty}</p>
          ) : (
            <div className="st-add__running">
              <RadioGroup<string>
                legend={t.running}
                value={picked?.token ?? null}
                options={running.map((program) => ({
                  value: program.token,
                  label: <span className="ow-mono">{program.exe}</span>,
                  description: program.folder ?? t.folderUnknown,
                }))}
                onChange={(token) => {
                  const program = running.find((p) => p.token === token);
                  if (program !== undefined) choose({ kind: "picked", program });
                }}
              />
            </div>
          )}
          <div className="st-add__typed">
            <label htmlFor={ids.typed} className="ow-field__label">
              {t.typeName}
            </label>
            <div className="ow-row">
              <input
                id={ids.typed}
                type="text"
                className="ow-input st-add__input"
                value={typed}
                autoComplete="off"
                spellCheck={false}
                aria-describedby={ids.typedHint}
                aria-invalid={typedTouched && typedProblem !== null}
                onChange={(event) => {
                  const value = event.currentTarget.value;
                  setTyped(value);
                  // A typed name counts without "Use this name": its name
                  // is offered as the game's name too.
                  if (chosen === null && !labelEdited && exeNameProblem(value) === null) {
                    setLabel(labelFromExe(typedProgramName(value)));
                  }
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    applyTyped();
                  }
                }}
              />
              <Button size="sm" onClick={applyTyped}>
                {t.useName}
              </Button>
            </div>
            <p
              id={ids.typedHint}
              className={`ow-field__notice${typedTouched && typedProblem === "folder" ? " ow-field__notice--error" : ""}`}
            >
              {typedTouched && typedProblem === "folder" ? t.typeNameInvalid : t.typeNameHint}
            </p>
          </div>
        </section>
        <div className="st-add__fields">
          <div>
            <label htmlFor={ids.name} className="ow-field__label">
              {t.name}
            </label>
            <input
              id={ids.name}
              type="text"
              className="ow-input st-add__input"
              value={label}
              maxLength={LABEL_MAX}
              autoComplete="off"
              aria-describedby={ids.nameHint}
              onChange={(event) => {
                setLabel(event.currentTarget.value);
                setLabelEdited(true);
              }}
            />
            <p id={ids.nameHint} className="ow-field__notice">
              {t.nameHint}
            </p>
          </div>
          <div>
            <label htmlFor={ids.profile} className="ow-field__label">
              {t.profile}
            </label>
            <select
              id={ids.profile}
              className="ow-input"
              value={profile}
              onChange={(event) => {
                setProfile(Number(event.currentTarget.value));
              }}
            >
              {PROFILES.map((n) => (
                <option key={n} value={n}>
                  {profileLabel(dict, n, names[n - 1] ?? null)}
                </option>
              ))}
            </select>
          </div>
        </div>
        {error === null ? null : (
          <div className="ow-stack">
            <p className="ow-strong">{t.failed}</p>
            <ErrorView error={error} written="none" />
          </div>
        )}
      </div>
    </Dialog>
  );
}
