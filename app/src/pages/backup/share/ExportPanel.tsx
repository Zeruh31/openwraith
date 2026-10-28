// Export a profile: choose where it is read from (the keyboard now, a backup
// file or an automatic backup), the profile, the parts the file holds (dead
// band and SOCD key pairs off at first) and an optional name, then the file
// in the system save dialog (the backend hands back a token, never a path).
// From the keyboard, the active profile is a pure read; another profile is
// read by switching to it and back, which the user agrees to first (with the
// vendor-site confirmation, as every switch). The backend is the authority
// and asks again (`needs_switch_consent`) when the active profile isn't what
// the page showed. A file source never touches the keyboard. Nothing is
// written to the keyboard's settings.

import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { Button } from "../../../components/Button";
import { Checkbox, RadioGroup, type ChoiceOption } from "../../../components/Controls";
import { ConfirmDialog } from "../../../components/Dialog";
import { Callout, Spinner } from "../../../components/Feedback";
import { Icon } from "../../../components/Icon";
import { ErrorView } from "../../../flow/ErrorView";
import { useVendorConfirmation, VendorNote } from "../../../flow/vendorTick";
import { OperationRunner } from "../../../flow/WriteFlow";
import { useDict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type {
  AppError,
  AutoBackupEntry,
  BackupFileSummary,
  ExportedProfile,
  ExportIntent,
  FileRef,
  FileToken,
  ShareSelectionView,
} from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { toastError } from "../../../shell/actions";
import { isLocked } from "../../../state/operation";
import { statusStore, useStatus } from "../../../state/status";
import { useDeviceBlock } from "../availability";
import { SavedFile } from "../BackupCard";
import { useDemoAction, type BackupDemo } from "../demo";
import { KEYBOARD_PROFILES } from "../model";
import {
  automaticOptionText,
  buildExportIntent,
  chosenParts,
  defaultExportInclude,
  defaultInto,
  EXPORT_SOURCES,
  exportFileProfile,
  exportNeedsSwitch,
  LABEL_MAX,
  SHARE_PARTS,
  withPart,
  type ExportFrom,
} from "./model";
import { ShareContents } from "./ShareContents";

/** A saved export, as the result dialog shows it. */
export function ExportedView({ exported }: { exported: ExportedProfile }) {
  const dict = useDict();
  const share = dict.backupRestore.share;
  const t = share.export;
  return (
    <div className="ow-report br-saved">
      <Callout kind="success" title={t.saved}>
        <p>{t.savedSummary({ n: exported.profile, firmware: exported.firmware ?? dict.common.unknown })}</p>
        {exported.switched ? <p>{t.switched({ n: exported.profile })}</p> : null}
      </Callout>
      <SavedFile file={exported.file} label={t.file} />
      <ShareContents contents={exported.contents} title={share.contents.title} />
      <p className="ow-muted ow-small">{share.noSerial}</p>
    </div>
  );
}

/** An export waiting for its file or its switch consent. */
interface Pending {
  profile: number;
  name: string;
  destination: FileRef;
  /** The backup the profile is taken from; `null` reads the keyboard. */
  file: FileToken | null;
  include: ShareSelectionView;
}

/** The chosen backup file (a read of the file only). */
interface ChosenFile {
  summary: BackupFileSummary;
  /** The profile of the file to export (`null`: the file holds none). */
  profile: number | null;
}

/** The automatic backups offered as a source. */
type AutoList = { s: "loading" } | { s: "ok"; entries: AutoBackupEntry[] } | { s: "error"; error: AppError };

/** What an export starts from: the profile and where it is read. */
interface Target {
  profile: number;
  file: FileToken | null;
}

/**
 * The automatic backups, listed each time that source is chosen (newest
 * first; the previous list stays until the new one arrives).
 */
function useAutoBackups(active: boolean): AutoList {
  const [list, setList] = useState<AutoList>({ s: "loading" });
  useEffect(() => {
    if (!active) return undefined;
    let alive = true;
    api.listAutoBackups().then(
      (view) => {
        if (alive) setList({ s: "ok", entries: view.entries });
      },
      (error: unknown) => {
        if (alive) setList({ s: "error", error: toAppError(error) });
      },
    );
    return () => {
      alive = false;
    };
  }, [active]);
  return list;
}

/** Props of {@link ExportPanel}. */
export interface ExportPanelProps {
  demo: BackupDemo | undefined;
}

/** The "Export a profile" half of "Share a profile". */
export function ExportPanel({ demo }: ExportPanelProps) {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const block = useDeviceBlock();
  const vendor = useVendorConfirmation();
  const share = dict.backupRestore.share;
  const t = share.export;
  const headingId = useId();
  const profileId = useId();
  const fileProfileId = useId();
  const automaticId = useId();
  const nameId = useId();
  const nameHintId = useId();
  const partsHintId = useId();
  const active = status.active_profile;
  const [from, setFrom] = useState<ExportFrom>(demo?.exportFrom ?? "keyboard");
  // `null` follows the active profile until the user picks one.
  const [picked, setPicked] = useState<number | null>(null);
  const [chosenFile, setChosenFile] = useState<ChosenFile | null>(null);
  const [fileError, setFileError] = useState<AppError | null>(null);
  const autos = useAutoBackups(from === "automatic");
  const [autoToken, setAutoToken] = useState<FileToken | null>(null);
  const [include, setInclude] = useState<ShareSelectionView>(defaultExportInclude);
  const [name, setName] = useState("");
  const [consent, setConsent] = useState<Pending | null>(null);
  const [running, setRunning] = useState<{ id: number; pending: Pending; intent: ExportIntent } | null>(null);
  const runs = useRef(0);
  const choosing = useRef(false);
  const dialogOpen = useRef(false);

  const keyboardProfile = picked ?? defaultInto(active);
  const entries = autos.s === "ok" ? autos.entries : [];
  // The chosen automatic backup, else the newest one.
  const autoEntry = entries.find((entry) => entry.file.token === autoToken) ?? entries[0] ?? null;

  let target: Target | null;
  switch (from) {
    case "keyboard":
      target = { profile: keyboardProfile, file: null };
      break;
    case "file":
      target =
        chosenFile === null || chosenFile.profile === null
          ? null
          : { profile: chosenFile.profile, file: chosenFile.summary.file.token };
      break;
    case "automatic":
      target = autoEntry === null ? null : { profile: autoEntry.profile, file: autoEntry.file.token };
      break;
  }
  // A file source doesn't need the keyboard, only a backend that isn't busy.
  const busy = status.busy !== null ? dict.backupRestore.reasons.busy : null;
  const sourceReason =
    from === "keyboard" ? block : target === null ? (from === "file" ? t.reasons.noFile : t.reasons.noAutomatic) : busy;
  const reason = sourceReason ?? (chosenParts(include).length === 0 ? t.reasons.noPart : null);

  const run = (pending: Pending, confirmedSwitch: boolean) => {
    runs.current += 1;
    setRunning({
      id: runs.current,
      pending,
      intent: buildExportIntent({
        profile: pending.profile,
        name: pending.name,
        destination: pending.destination.token,
        confirmedSwitch,
        file: pending.file,
        include: pending.include,
      }),
    });
  };

  /** Asks for the file, then for the switch (when another profile is read from the keyboard), then exports. */
  const start = (chosen: Target) => {
    if (choosing.current || running !== null || consent !== null || reason !== null || isLocked()) return;
    choosing.current = true;
    const label = name;
    const parts = include;
    api
      .chooseExportDestination(chosen.profile, { title: share.dialog.save, filter: share.dialog.saveFilter })
      .then(
        (destination) => {
          if (destination === null) return;
          const pending: Pending = { profile: chosen.profile, name: label, destination, file: chosen.file, include: parts };
          // The status of now, not of the render that started the dialog.
          if (chosen.file === null && exportNeedsSwitch(chosen.profile, statusStore.get().active_profile)) {
            setConsent(pending);
          } else {
            run(pending, false);
          }
        },
        (error: unknown) => {
          toastError(error);
        },
      )
      .finally(() => {
        choosing.current = false;
      });
  };

  /** Opens a backup file in the system dialog (a read of the file only). */
  const chooseFile = () => {
    if (dialogOpen.current) return;
    dialogOpen.current = true;
    api
      .openBackupFile({ title: share.dialog.openBackup, filter: dict.backupRestore.dialog.filter })
      .then(
        (summary) => {
          if (summary === null) return;
          setChosenFile({ summary, profile: exportFileProfile(summary) });
          setFileError(null);
        },
        (error: unknown) => {
          setChosenFile(null);
          setFileError(toAppError(error));
        },
      )
      .finally(() => {
        dialogOpen.current = false;
      });
  };

  useDemoAction(demo?.share === "export", () => {
    if (from === "keyboard") {
      const profile = demo?.exportProfile ?? keyboardProfile;
      setPicked(profile);
      start({ profile, file: null });
    } else if (target !== null) {
      start(target);
    }
  });

  const sources: ChoiceOption<ExportFrom>[] = EXPORT_SOURCES.map((source) => ({
    value: source,
    label: t.sources[source],
    description: t.sourceHints[source],
  }));
  const switching = from === "keyboard" && active !== null && exportNeedsSwitch(keyboardProfile, active);
  const unknown = dict.common.unknown;

  let sourceFields: ReactNode;
  switch (from) {
    case "keyboard":
      sourceFields = (
        <div className="br-share__field">
          <label htmlFor={profileId} className="ow-field__label">
            {t.profile}
          </label>
          <select
            id={profileId}
            className="ow-input br-share__select"
            value={keyboardProfile}
            onChange={(event) => {
              setPicked(Number(event.currentTarget.value));
            }}
          >
            {KEYBOARD_PROFILES.map((n) => (
              <option key={n} value={n}>
                {share.profileOption({ n, active: n === active })}
              </option>
            ))}
          </select>
        </div>
      );
      break;
    case "file": {
      const summary = chosenFile?.summary ?? null;
      sourceFields = (
        <div className="br-share__source">
          <div className="ow-row">
            <Button icon="folder" variant={summary === null ? "primary" : "secondary"} onClick={chooseFile}>
              {summary === null ? t.chooseFile : t.chooseOther}
            </Button>
          </div>
          {fileError === null ? null : <ErrorView error={fileError} written="none" />}
          {summary === null || chosenFile === null ? null : (
            <>
              <div className="br-share__picked">
                <p className="br-file__name ow-mono ow-break ow-selectable">{summary.file.file_name}</p>
                <p className="ow-muted ow-small">
                  {t.fileMade({ date: format.dateTime(summary.created_at), firmware: summary.firmware ?? unknown })}
                </p>
              </div>
              {summary.profiles.length === 0 ? null : (
                <div className="br-share__field">
                  <label htmlFor={fileProfileId} className="ow-field__label">
                    {t.fileProfile}
                  </label>
                  <select
                    id={fileProfileId}
                    className="ow-input br-share__select"
                    value={chosenFile.profile ?? ""}
                    onChange={(event) => {
                      const profile = Number(event.currentTarget.value);
                      setChosenFile((current) => (current === null ? null : { ...current, profile }));
                    }}
                  >
                    {summary.profiles.map((n) => (
                      <option key={n} value={n}>
                        {dict.common.profile({ n })}
                      </option>
                    ))}
                  </select>
                </div>
              )}
            </>
          )}
        </div>
      );
      break;
    }
    case "automatic":
      switch (autos.s) {
        case "loading":
          sourceFields = (
            <p className="ow-row" role="status">
              <Spinner />
              <span>{dict.common.loading}</span>
            </p>
          );
          break;
        case "error":
          sourceFields = (
            <>
              <p>{t.automaticFailed}</p>
              <ErrorView error={autos.error} written="none" />
            </>
          );
          break;
        case "ok":
          sourceFields =
            entries.length === 0 ? (
              <p className="ow-muted">{t.noAutomatic}</p>
            ) : (
              <div className="br-share__field br-share__field--wide">
                <label htmlFor={automaticId} className="ow-field__label">
                  {t.automatic}
                </label>
                <select
                  id={automaticId}
                  className="ow-input br-share__select"
                  value={autoEntry?.file.token ?? ""}
                  onChange={(event) => {
                    setAutoToken(event.currentTarget.value);
                  }}
                >
                  {entries.map((entry) => (
                    <option key={entry.file.token} value={entry.file.token}>
                      {automaticOptionText(entry, dict, format)}
                    </option>
                  ))}
                </select>
              </div>
            );
          break;
      }
      break;
  }

  const shownProfile = target?.profile ?? keyboardProfile;
  return (
    <section className="br-share__panel" aria-labelledby={headingId}>
      <h3 id={headingId} className="br-share__title">
        <Icon name="backup" size={18} className="br-share__icon" />
        <span>{t.title}</span>
      </h3>
      <p className="ow-muted">{t.intro}</p>
      <RadioGroup<ExportFrom>
        legend={t.source}
        options={sources}
        value={from}
        onChange={(next) => {
          setFrom(next);
        }}
      />
      <div className="br-share__fields">{sourceFields}</div>
      <div className="br-share__fields">
        <div className="br-share__field br-share__field--wide">
          <label htmlFor={nameId} className="ow-field__label">
            {t.name}
          </label>
          <input
            id={nameId}
            type="text"
            className="ow-input br-share__name"
            value={name}
            maxLength={LABEL_MAX}
            autoComplete="off"
            spellCheck={false}
            aria-describedby={nameHintId}
            onChange={(event) => {
              setName(event.currentTarget.value);
            }}
          />
          <p id={nameHintId} className="ow-muted ow-small">
            {t.nameHint}
          </p>
        </div>
      </div>
      <fieldset className="br-share__choice" aria-describedby={partsHintId}>
        <legend className="ow-field__label">{t.parts}</legend>
        <p id={partsHintId} className="ow-muted ow-small">
          {t.partsHint}
        </p>
        <div className="br-share__checks">
          {SHARE_PARTS.map((part) => (
            <Checkbox
              key={part}
              checked={include[part]}
              label={share.parts[part]}
              onChange={(checked) => {
                setInclude((current) => withPart(current, part, checked));
              }}
            />
          ))}
        </div>
      </fieldset>
      <p className="ow-muted ow-small">{t.holds}</p>
      {include.keys ? <p className="ow-muted ow-small">{t.macrosNote}</p> : null}
      {switching ? <p className="ow-muted ow-small">{t.switchNote({ n: keyboardProfile })}</p> : null}
      <div className="ow-row">
        <Button
          variant="primary"
          icon="backup"
          disabledReason={reason}
          onClick={() => {
            if (target !== null) start(target);
          }}
        >
          {t.button({ n: shownProfile })}
        </Button>
      </div>
      <ConfirmDialog
        open={consent !== null}
        title={t.consentTitle}
        confirmLabel={t.consentConfirm}
        onCancel={() => {
          setConsent(null);
        }}
        footer={<VendorNote confirmation={vendor} />}
        disabledReason={vendor.missing ? dict.flow.review.vendorTickNeeded : null}
        onConfirm={() => {
          if (vendor.missing || consent === null) return;
          vendor.acknowledge();
          const pending = consent;
          setConsent(null);
          run(pending, true);
        }}
      >
        {consent === null ? null : (
          <p>
            {active === null
              ? t.consentBodyUnknown({ n: consent.profile })
              : t.consentBody({ n: consent.profile, active })}
          </p>
        )}
      </ConfirmDialog>
      {running === null ? null : (
        <OperationRunner<ExportedProfile>
          key={running.id}
          kind="export"
          title={dict.flow.opKinds.export}
          run={(onProgress) =>
            api.exportProfile(running.intent, onProgress).catch((error: unknown) => {
              const appError = toAppError(error);
              if (appError.detail?.detail === "needs_switch_consent" && running.pending.file === null) {
                // The keyboard is on another profile than shown: ask for the switch first.
                setRunning(null);
                setConsent(running.pending);
              }
              throw appError;
            })
          }
          result={(exported) => <ExportedView exported={exported} />}
          errorNote={() => t.noFileSaved}
          written={running.pending.file === null ? "switches" : "nothing"}
          onClose={() => {
            setRunning(null);
          }}
        />
      )}
    </section>
  );
}
