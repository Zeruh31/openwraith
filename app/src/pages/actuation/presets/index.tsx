// Actuation > Trigger: the "Presets" menu. It lists the library's built-in
// presets and the user's presets (stored in the settings of this PC, checked
// against the keyboard's limits: a preset out of range is shown, refused,
// never clamped). Choosing a preset and its keys (all, WASD, the selection)
// fills the trigger tab's draft and opens the review, which the backend
// plans from the preset itself (`prepare_preset_edit`). "Save as a preset"
// stores the current values under a name. Nothing here writes to the
// keyboard.

import { useId, useState } from "react";

import { Button, IconButton } from "../../../components/Button";
import { RadioGroup, type ChoiceOption } from "../../../components/Controls";
import { Dialog } from "../../../components/Dialog";
import { Callout, Spinner } from "../../../components/Feedback";
import { TechnicalDetails } from "../../../components/Layout";
import { useDict, type Dict } from "../../../i18n";
import { useFormat, type Format } from "../../../i18n/format";
import type { AppError, KeyTriggerView, PresetRef, PresetsView } from "../../../ipc/bindings";
import { api } from "../../../ipc/commands";
import { toAppError } from "../../../ipc/errors";
import { toast } from "../../../state/toasts";
import { EditRefusal } from "../shared/EditParts";
import { editProblemOf, editProblemText } from "../shared/editTexts";
import { valueTexts, type TriggerDraft } from "../model";
import {
  cleanName,
  MAX_NAME,
  presetDraft,
  presetItems,
  presetKey,
  presetSetting,
  targetKeys,
  valuesToSave,
  type PresetItem,
  type PresetTarget,
} from "./model";

/** Props of {@link PresetsMenu}. */
export interface PresetsMenuProps {
  /** The keys of the snapshot (index → settings). */
  keys: ReadonlyMap<number, KeyTriggerView>;
  selection: ReadonlySet<number>;
  /** The draft (what "Save as a preset" saves when it sets both values). */
  draft: TriggerDraft;
  unitUm: number;
  /** Why a preset can't be applied now (not connected, read-only, stale, busy…), or `null`. */
  disabledReason: string | null;
  /** Fills the draft from the preset for `keys` and opens the review (`name`: the preset's name as shown). */
  onApply: (preset: PresetRef, draft: TriggerDraft, keys: ReadonlySet<number>, name: string) => void;
}

/** The name of a preset (built-in ones translated by id). */
export function presetName(item: PresetItem, dict: Dict): string {
  if (item.user !== null) return item.user.name;
  const names: Readonly<Record<string, string>> = dict.actuation.presets.builtin;
  return names[item.ref.id] ?? item.ref.id;
}

/** Why saving a preset failed (a refused value in the page's words). */
function saveErrorText(error: AppError, dict: Dict, format: Format, unitUm: number): string {
  const problem = editProblemOf(error);
  return problem === null ? dict.errors.kind[error.kind].body : editProblemText(problem, dict, format, unitUm);
}

type Load = { s: "loading" } | { s: "ready"; view: PresetsView } | { s: "error"; error: AppError };

/** The "Presets…" button and its dialog. */
export default function PresetsMenu({ keys, selection, draft, unitUm, disabledReason, onApply }: PresetsMenuProps) {
  const dict = useDict();
  const format = useFormat();
  const t = dict.actuation.presets;
  const texts = valueTexts(dict, format, unitUm);
  const nameId = useId();
  const [open, setOpen] = useState(false);
  const [load, setLoad] = useState<Load>({ s: "loading" });
  const [chosen, setChosen] = useState<string | null>(null);
  const [target, setTarget] = useState<PresetTarget>(selection.size > 0 ? "selection" : "wasd");
  const [name, setName] = useState("");
  const [saveError, setSaveError] = useState<AppError | null>(null);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);

  const refresh = () => {
    setLoad({ s: "loading" });
    api
      .listPresets()
      .then((view) => {
        setLoad({ s: "ready", view });
      })
      .catch((error: unknown) => {
        setLoad({ s: "error", error: toAppError(error) });
      });
  };

  const show = () => {
    setOpen(true);
    setChosen(null);
    setSaveError(null);
    setTarget(selection.size > 0 ? "selection" : "wasd");
    refresh();
  };

  const items = load.s === "ready" ? presetItems(load.view) : [];
  const item = items.find((candidate) => candidate.key === chosen) ?? null;
  const counts: Record<PresetTarget, number> = {
    all: targetKeys("all", keys, selection).length,
    wasd: targetKeys("wasd", keys, selection).length,
    selection: targetKeys("selection", keys, selection).length,
  };
  const toSave = valuesToSave(draft, selection, keys);
  const cleaned = cleanName(name);

  let reviewReason: string | null = disabledReason;
  if (reviewReason === null && item === null) reviewReason = t.reasons.choose;
  if (reviewReason === null && counts[target] === 0) reviewReason = t.reasons.noKeys;

  const review = () => {
    if (item === null || reviewReason !== null) return;
    const chosenKeys = new Set(targetKeys(target, keys, selection));
    setOpen(false);
    onApply(item.ref, presetDraft(item.actuation, item.rt), chosenKeys, presetName(item, dict));
  };

  const save = () => {
    if (toSave === null || cleaned === "" || saving) return;
    setSaving(true);
    setSaveError(null);
    api
      .saveUserPreset({ id: null, name: cleaned, ...toSave })
      .then((view) => {
        setLoad({ s: "ready", view });
        setName("");
        toast({ kind: "success", title: t.saved({ name: cleaned }) });
      })
      .catch((error: unknown) => {
        setSaveError(toAppError(error));
      })
      .finally(() => {
        setSaving(false);
      });
  };

  const remove = (id: string, label: string) => {
    if (deleting !== null) return;
    setDeleting(id);
    api
      .deleteUserPreset(id)
      .then((view) => {
        setLoad({ s: "ready", view });
        if (chosen === presetKey({ kind: "user", id })) setChosen(null);
        toast({ kind: "info", title: t.deleted({ name: label }) });
      })
      .catch((error: unknown) => {
        setSaveError(toAppError(error));
      })
      .finally(() => {
        setDeleting(null);
      });
  };

  const option = (entry: PresetItem): ChoiceOption<string> => {
    const refused = entry.user?.refused ?? null;
    const values = presetSetting(entry.actuation, entry.rt, texts);
    const hints: Readonly<Record<string, string>> = t.builtinHints;
    const hint = entry.user === null ? hints[entry.ref.id] : undefined;
    return {
      value: entry.key,
      label: presetName(entry, dict),
      description:
        refused === null
          ? [values, hint].filter(Boolean).join(" · ")
          : `${values} · ${t.refused({ why: editProblemText(refused, dict, format, unitUm) })}`,
      disabled: refused !== null,
    };
  };

  const view = load.s === "ready" ? load.view : null;
  const builtin = items.filter((entry) => entry.user === null);
  const user = items.filter((entry) => entry.user !== null);
  const targetOptions: ChoiceOption<PresetTarget>[] = [
    { value: "all", label: t.targets.all({ n: counts.all }) },
    { value: "wasd", label: t.targets.wasd, disabled: counts.wasd === 0 },
    { value: "selection", label: t.targets.selection({ n: counts.selection }), disabled: counts.selection === 0 },
  ];
  const full = user.length >= (view?.max_user ?? Number.POSITIVE_INFINITY);
  let saveReason: string | null = null;
  if (toSave === null) saveReason = t.save.nothing;
  else if (cleaned === "") saveReason = t.save.nameNeeded;
  else if (full) saveReason = t.save.full({ max: view?.max_user ?? 0 });

  return (
    <>
      <Button size="sm" onClick={show} aria-haspopup="dialog">
        {t.open}
      </Button>
      <Dialog
        open={open}
        title={t.title}
        size="md"
        onClose={() => {
          setOpen(false);
        }}
        actions={
          <>
            <Button
              onClick={() => {
                setOpen(false);
              }}
            >
              {dict.common.actions.cancel}
            </Button>
            <Button variant="primary" disabledReason={reviewReason} onClick={review}>
              {dict.common.actions.reviewAndWrite}
            </Button>
          </>
        }
      >
        <div className="ow-stack act-presets">
          <p>{t.intro}</p>
          {load.s === "loading" ? (
            <p className="ow-row">
              <Spinner />
              <span>{t.loading}</span>
            </p>
          ) : null}
          {load.s === "error" ? <EditRefusal error={load.error} unitUm={unitUm} /> : null}
          {view === null ? null : (
            <>
              <RadioGroup<string>
                legend={t.builtinTitle}
                options={builtin.map(option)}
                value={chosen}
                onChange={setChosen}
              />
              {user.length === 0 ? (
                <p className="ow-muted ow-small">{t.noUser}</p>
              ) : (
                <RadioGroup<string> legend={t.userTitle} options={user.map(option)} value={chosen} onChange={setChosen} />
              )}
              <RadioGroup<PresetTarget> legend={t.targetTitle} options={targetOptions} value={target} onChange={setTarget} />
              <p className="ow-muted ow-small">{t.reviewNote}</p>
              <section className="act-presets__save" aria-labelledby={`${nameId}-h`}>
                <h3 id={`${nameId}-h`} className="act-inspector__title">
                  {t.save.title}
                </h3>
                <p className="ow-small">
                  {toSave === null
                    ? t.save.nothing
                    : t.save.what({ values: presetSetting(toSave.actuation, toSave.rapid_trigger, texts) })}
                </p>
                <div className="act-presets__save-row">
                  <div className="act-presets__name">
                    <label htmlFor={nameId} className="ow-field__label">
                      {t.save.name}
                    </label>
                    <input
                      id={nameId}
                      type="text"
                      className="ow-input"
                      maxLength={MAX_NAME}
                      value={name}
                      disabled={toSave === null}
                      autoComplete="off"
                      onChange={(event) => {
                        setName(event.currentTarget.value);
                      }}
                    />
                  </div>
                  <Button disabledReason={saving ? null : saveReason} disabled={saving} onClick={save}>
                    {t.save.button}
                  </Button>
                </div>
                {saveError === null ? null : (
                  <Callout kind="danger" title={t.save.refused} role="alert">
                    <p>{saveErrorText(saveError, dict, format, unitUm)}</p>
                    {saveError.message === "" ? null : <TechnicalDetails>{saveError.message}</TechnicalDetails>}
                  </Callout>
                )}
                {user.length === 0 ? null : (
                  <ul className="act-presets__mine" aria-label={t.userTitle}>
                    {user.map((entry) => {
                      const label = presetName(entry, dict);
                      return (
                        <li key={entry.key}>
                          <span>{label}</span>
                          <IconButton
                            icon="trash"
                            size="sm"
                            label={t.delete({ name: label })}
                            disabled={deleting !== null}
                            onClick={() => {
                              remove(entry.ref.id, label);
                            }}
                          />
                        </li>
                      );
                    })}
                  </ul>
                )}
              </section>
            </>
          )}
        </div>
      </Dialog>
    </>
  );
}
