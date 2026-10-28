// Import a profile: open a profile file (or a backup) in the system dialog
// (a read of the file only; the backend hands back a token, never a path),
// see what it holds (macros as the keys they press), choose the target
// profile and the parts, then compare and review (ImportFlow). Comparing
// another profile than the active one switches to it and back, which the
// user agrees to first (with the vendor-site confirmation); the backend is
// the authority and asks again when its fresh read differs.

import { useId, useRef, useState } from "react";

import { Button, IconButton } from "../../../components/Button";
import { Checkbox } from "../../../components/Controls";
import { ConfirmDialog } from "../../../components/Dialog";
import { Callout } from "../../../components/Feedback";
import { Icon } from "../../../components/Icon";
import { KeyValueList, type KeyValueItem } from "../../../components/Layout";
import { ErrorView } from "../../../flow/ErrorView";
import { useVendorConfirmation, VendorNote } from "../../../flow/vendorTick";
import { useDict } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type { AppError, ImportIntent, ImportSummaryView, ShareSelectionView } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { toastError } from "../../../shell/actions";
import { isLocked } from "../../../state/operation";
import { useStatus } from "../../../state/status";
import { useDeviceBlock } from "../availability";
import { useDemoAction, type BackupDemo } from "../demo";
import { KEYBOARD_PROFILES } from "../model";
import { numberList, profilesLabel } from "../texts";
import { ImportFlow, type FirmwareSeen } from "./ImportFlow";
import {
  buildImportIntent,
  chosenParts,
  defaultFrom,
  defaultInto,
  fileProfiles,
  importSwitches,
  includeOf,
  initialInclude,
  SHARE_PARTS,
  withPart,
  type SharePart,
} from "./model";
import { ShareContents } from "./ShareContents";

/** The opened file's facts and what its profile holds (the summary card). */
function ImportFileSummary({ summary, from }: { summary: ImportSummaryView; from: number | null }) {
  const dict = useDict();
  const format = useFormat();
  const s = dict.backupRestore.share;
  const t = s.import;
  const file = summary.file;
  const unknown = dict.common.unknown;
  const model = file.model ?? file.product ?? unknown;
  const items: KeyValueItem[] = [
    { key: "made", label: t.facts.made, value: <span className="ow-num">{format.dateTime(file.created_at)}</span> },
    { key: "keyboard", label: t.facts.keyboard, value: model },
    { key: "firmware", label: t.facts.firmware, value: <span className="ow-num">{file.firmware ?? unknown}</span> },
    { key: "profiles", label: t.facts.profiles, value: profilesLabel(file.profiles, dict, format) },
    { key: "tool", label: t.facts.tool, value: <span className="ow-mono">{`${file.tool.name} ${file.tool.version}`}</span> },
  ];
  return (
    <div className="br-file">
      <div className="br-file__head">
        <Icon name="profile" size={20} className="br-file__icon" />
        <div className="br-file__names">
          <p className="br-file__name ow-mono ow-break ow-selectable">{file.file.file_name}</p>
          <p className="ow-muted ow-small ow-mono ow-break ow-selectable">{file.file.display_path}</p>
        </div>
        <IconButton
          icon="folder"
          size="sm"
          label={dict.common.actions.showInFolder}
          onClick={() => {
            api.reveal({ target: "file", file: file.file.token }).catch(toastError);
          }}
        />
      </div>
      <KeyValueList items={items} />
      {file.made_by_simulator ? <Callout kind="warning">{t.simulator}</Callout> : null}
      {file.same_model === false ? (
        <Callout kind="danger" title={t.otherKeyboardTitle} role="alert">
          <p>{t.otherKeyboardBody({ model })}</p>
        </Callout>
      ) : null}
      {file.same_model === null ? <p className="ow-muted ow-small">{t.notChecked}</p> : null}
      {from === summary.profile || summary.profile === null ? (
        <ShareContents
          contents={summary.contents}
          title={summary.profile === null ? s.contents.title : s.contents.titleOf({ n: summary.profile })}
        />
      ) : (
        <p className="ow-muted">{s.contents.otherProfile({ n: from ?? 1 })}</p>
      )}
    </div>
  );
}

/** One part's checkbox; a part the file lacks is disabled with the note outside the dimmed box. */
function PartChoice({
  part,
  present,
  checked,
  onChange,
}: {
  part: SharePart;
  present: boolean;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  const dict = useDict();
  const s = dict.backupRestore.share;
  if (!present) {
    return (
      <div className="br-profile-off">
        <Checkbox checked={false} disabled label={s.parts[part]} onChange={onChange} />
        <p className="ow-muted ow-small">{s.import.notInFile}</p>
      </div>
    );
  }
  const hint = part === "keys" ? s.import.keysHint : part === "socd" ? s.import.socdHint : undefined;
  return <Checkbox checked={checked} label={s.parts[part]} description={hint} onChange={onChange} />;
}

/** Props of {@link ImportSteps}. */
interface ImportStepsProps {
  summary: ImportSummaryView;
  demo: BackupDemo | undefined;
  onImported: () => void;
}

/** The choices for one opened file and the import flow (remounted for every new file). */
function ImportSteps({ summary, demo, onImported }: ImportStepsProps) {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const block = useDeviceBlock();
  // Comparing another profile switches profiles: the same vendor-site
  // confirmation as every write.
  const vendor = useVendorConfirmation();
  const s = dict.backupRestore.share;
  const t = s.import;
  const fromId = useId();
  const intoId = useId();
  const intoHintId = useId();
  const partsId = useId();
  const profiles = fileProfiles(summary);
  const [from, setFrom] = useState<number | null>(() => defaultFrom(summary));
  const [into, setInto] = useState(() => demo?.into ?? defaultInto(status.active_profile));
  const [include, setInclude] = useState<ShareSelectionView>(() => {
    const initial = initialInclude(summary);
    return demo?.keys === true ? withPart(initial, "keys", summary.contents.present.keys) : initial;
  });
  const [allowFirmware, setAllowFirmware] = useState(false);
  const [firmwareSeen, setFirmwareSeen] = useState<FirmwareSeen | null>(null);
  const [consent, setConsent] = useState<number[] | null>(null);
  const [flow, setFlow] = useState<{ id: number; intent: ImportIntent } | null>(null);
  const flows = useRef(0);

  const present = summary.contents.present;
  const chosen = includeOf(include, present);
  const firmwareDiffers = summary.file.same_firmware === false || firmwareSeen !== null;
  const reason =
    block ??
    (summary.file.same_model === false
      ? s.reasons.otherKeyboard
      : chosenParts(chosen).length === 0
        ? s.reasons.noPart
        : firmwareDiffers && !allowFirmware
          ? s.reasons.firmwareTick
          : null);

  const openFlow = (switches: readonly number[]) => {
    flows.current += 1;
    setFlow({
      id: flows.current,
      intent: buildImportIntent({
        source: summary.file.file.token,
        from,
        fileProfile: summary.profile,
        into,
        include: chosen,
        allowedCritical: [],
        allowFirmware,
        switches,
      }),
    });
  };

  /** Asks for the profile switch (when there is one), then compares. */
  const begin = () => {
    const switches = importSwitches(into, status.active_profile);
    setFlow(null);
    if (switches.length > 0) {
      setConsent(switches);
    } else {
      openFlow([]);
    }
  };

  const onReview = () => {
    if (reason !== null || flow !== null || consent !== null || isLocked()) return;
    begin();
  };

  useDemoAction(demo?.share === "import", onReview);

  const unknown = dict.common.unknown;
  const active = status.active_profile;
  return (
    <>
      <ImportFileSummary summary={summary} from={from} />
      {firmwareDiffers ? (
        <Callout kind="warning" title={t.firmwareTitle}>
          <p>
            {t.firmwareBody({
              file: summary.file.firmware ?? firmwareSeen?.file ?? unknown,
              keyboard: status.device?.firmware ?? firmwareSeen?.keyboard ?? unknown,
            })}
          </p>
          <Checkbox
            checked={allowFirmware}
            onChange={setAllowFirmware}
            label={t.firmwareAnyway}
            description={t.firmwareAnywayHint}
          />
        </Callout>
      ) : null}
      <div className="br-share__fields">
        {profiles.length > 1 ? (
          <div className="br-share__field">
            <label htmlFor={fromId} className="ow-field__label">
              {t.from}
            </label>
            <select
              id={fromId}
              className="ow-input br-share__select"
              value={from ?? ""}
              onChange={(event) => {
                setFrom(Number(event.currentTarget.value));
              }}
            >
              {profiles.map((n) => (
                <option key={n} value={n}>
                  {dict.common.profile({ n })}
                </option>
              ))}
            </select>
          </div>
        ) : null}
        <div className="br-share__field">
          <label htmlFor={intoId} className="ow-field__label">
            {t.into}
          </label>
          <select
            id={intoId}
            className="ow-input br-share__select"
            value={into}
            aria-describedby={into === active ? undefined : intoHintId}
            onChange={(event) => {
              setInto(Number(event.currentTarget.value));
            }}
          >
            {KEYBOARD_PROFILES.map((n) => (
              <option key={n} value={n}>
                {s.profileOption({ n, active: n === active })}
              </option>
            ))}
          </select>
          {into === active ? null : (
            <p id={intoHintId} className="ow-muted ow-small">
              {t.intoHint({ n: into })}
            </p>
          )}
        </div>
      </div>
      <fieldset className="br-share__choice" aria-describedby={partsId}>
        <legend className="ow-field__label">{t.parts}</legend>
        <p id={partsId} className="ow-muted ow-small">
          {t.partsHint}
        </p>
        <div className="br-share__checks">
          {SHARE_PARTS.map((part) => (
            <PartChoice
              key={part}
              part={part}
              present={present[part]}
              checked={chosen[part]}
              onChange={(checked) => {
                setInclude((current) => withPart(current, part, checked));
              }}
            />
          ))}
        </div>
      </fieldset>
      <p className="ow-muted">{t.compareHint}</p>
      <div className="ow-row">
        <Button variant="primary" icon="refresh" disabledReason={reason} onClick={onReview}>
          {t.review}
        </Button>
      </div>
      <ConfirmDialog
        open={consent !== null}
        title={dict.backupRestore.restore.consentTitle}
        confirmLabel={dict.backupRestore.restore.consentConfirm}
        onCancel={() => {
          setConsent(null);
        }}
        footer={<VendorNote confirmation={vendor} />}
        disabledReason={vendor.missing ? dict.flow.review.vendorTickNeeded : null}
        onConfirm={() => {
          if (vendor.missing) return;
          vendor.acknowledge();
          const switches = consent;
          setConsent(null);
          if (switches !== null) openFlow(switches);
        }}
      >
        {consent === null ? null : (
          <p>
            {active === null
              ? dict.backupRestore.restore.consentBodyUnknown({ list: numberList(consent, format), count: consent.length })
              : dict.backupRestore.restore.consentBody({
                  list: numberList(consent, format),
                  count: consent.length,
                  active,
                })}
          </p>
        )}
      </ConfirmDialog>
      {flow === null ? null : (
        <ImportFlow
          key={flow.id}
          intent={flow.intent}
          onConsent={(profiles) => {
            setFlow(null);
            setConsent(profiles);
          }}
          onFirmware={setFirmwareSeen}
          onImported={onImported}
          onRunAgain={begin}
          onClose={() => {
            setFlow(null);
          }}
        />
      )}
    </>
  );
}

/** Props of {@link ImportPanel}. */
export interface ImportPanelProps {
  demo: BackupDemo | undefined;
  /** Called after an import was applied (a new safety backup exists). */
  onImported: () => void;
}

interface Opened {
  summary: ImportSummaryView | null;
  /** Increases with every opened file (the choices start over). */
  loadId: number;
  error: AppError | null;
}

/** The "Import a profile" half of "Share a profile". */
export function ImportPanel({ demo, onImported }: ImportPanelProps) {
  const dict = useDict();
  const s = dict.backupRestore.share;
  const t = s.import;
  const headingId = useId();
  const [opened, setOpened] = useState<Opened>({ summary: null, loadId: 0, error: null });
  const dialogOpen = useRef(false);

  const open = () => {
    if (dialogOpen.current) return;
    dialogOpen.current = true;
    api
      .openImportFile({ title: s.dialog.open, filter: s.dialog.openFilter })
      .then(
        (summary) => {
          if (summary !== null) setOpened((current) => ({ summary, loadId: current.loadId + 1, error: null }));
        },
        (error: unknown) => {
          setOpened((current) => ({ ...current, summary: null, error: toAppError(error) }));
        },
      )
      .finally(() => {
        dialogOpen.current = false;
      });
  };

  useDemoAction(demo?.share === "open_import" || demo?.share === "import", open);

  const summary = opened.summary;
  return (
    <section className="br-share__panel" aria-labelledby={headingId}>
      <h3 id={headingId} className="br-share__title">
        <Icon name="restore" size={18} className="br-share__icon" />
        <span>{t.title}</span>
      </h3>
      <p className="ow-muted">{t.intro}</p>
      <div className="ow-row">
        <Button icon="folder" variant={summary === null ? "primary" : "secondary"} onClick={open}>
          {summary === null ? t.open : t.openOther}
        </Button>
      </div>
      {opened.error === null ? null : <ErrorView error={opened.error} written="none" />}
      {summary === null ? (
        opened.error === null ? (
          <p className="ow-muted ow-small">{t.backupsToo}</p>
        ) : null
      ) : (
        <ImportSteps key={opened.loadId} summary={summary} demo={demo} onImported={onImported} />
      )}
    </section>
  );
}
