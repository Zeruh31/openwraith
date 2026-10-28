// The editor of one advanced key, as a dialog (at most 760 px wide): the
// key it goes on (for a new one), then the kind's own editor:
// - dynamic keystroke (DKS): the first point (0.1 mm steps), the travel
//   gauge with its two points, quick starts and the grid of keys × stages;
// - tap or hold: the tap key, the hold key and the hold time;
// - toggle: the key.
// A polite live sentence says what the advanced key does after every
// change. Keys are chosen in the action picker (Keys tab only), shown in
// place of the editor, so no dialog opens on top of another. Saving only
// puts the change into the tab's draft; nothing is written from here.

import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { ACTION_CATALOG, ActionPicker, usageName } from "../../../components/actions";
import { EvidenceBadge } from "../../../components/Badge";
import { Button } from "../../../components/Button";
import { Dialog } from "../../../components/Dialog";
import { Slider } from "../../../components/Fields";
import { TravelGauge } from "../../../components/TravelGauge";
import { useDict, useLang } from "../../../i18n";
import { useFormat } from "../../../i18n/format";
import type {
  AdvancedDefView,
  BuiltinAckIntent,
  DksRowView,
  DksStepView,
  KeysState,
  LayerView,
} from "../../../ipc/bindings";
import { bindingsText, keyAt } from "../shared";
import { cellId, DksGrid } from "./DksGrid";
import {
  applyQuickStart,
  defProblem,
  defSentence,
  fourRows,
  holdTimeEvidence,
  newDef,
  ownUsage,
  QUICK_STARTS,
  quickStartEvidence,
  setStep,
  tenthsMm,
  type AdvancedType,
} from "./model";
import { needsConfirmation, TargetKey, type TargetValue } from "./TargetKey";

/** What the editor opens on. */
export type EditorRequest =
  /** A new advanced key; `item` re-edits one already in the draft. */
  | {
      mode: "create";
      type: AdvancedType;
      item: { id: string; layer: LayerView; key: number; def: AdvancedDefView } | null;
    }
  /** A slot's definition (`itemId`: the draft's edit of it, if any). */
  | { mode: "edit"; slot: number; def: AdvancedDefView; itemId: string | null };

/** What the editor saves. */
export interface EditorResult {
  def: AdvancedDefView;
  /** The key a new advanced key goes on. */
  target: { layer: LayerView; key: number } | null;
  /** The confirmation of a critical built-in the target replaces. */
  ack: BuiltinAckIntent | null;
}

/** Props of {@link AdvancedEditor}. */
export interface AdvancedEditorProps {
  state: KeysState;
  request: EditorRequest;
  /** Keys whose critical built-in the user already confirmed (a re-edited draft item). */
  confirmedKeys?: readonly number[] | undefined;
  onSave: (result: EditorResult) => void;
  onClose: () => void;
}

/** What the action picker is choosing a key for. */
type PickTarget =
  | { what: "row"; position: number | null }
  | { what: "tap" }
  | { what: "hold" }
  | { what: "toggle" };

type DksDef = Extract<AdvancedDefView, { type: "dks" }>;

function initialTarget(request: EditorRequest, confirmedKeys: readonly number[]): TargetValue {
  if (request.mode === "create" && request.item !== null) {
    return { layer: request.item.layer, key: request.item.key, confirmed: confirmedKeys.includes(request.item.key) };
  }
  return { layer: "base", key: null, confirmed: false };
}

function initialDef(request: EditorRequest): AdvancedDefView {
  if (request.mode === "edit") return request.def;
  return request.item?.def ?? newDef(request.type, null);
}

/** A key choice: its label, the key, and the button that opens the picker. */
function KeyField({ id, label, usage, onChange }: { id: string; label: string; usage: number; onChange: () => void }) {
  const dict = useDict();
  const name = usageName(usage, dict);
  return (
    <div className="keys-adv-keyfield">
      <span className="ow-field__label">{label}</span>
      <span className="keys-adv-keyfield__row">
        <span className="keys-adv-keyfield__value ow-strong">{name}</span>
        <Button
          id={id}
          size="sm"
          aria-label={dict.keysAdvanced.editor.changeKey({ what: label, key: name })}
          onClick={onChange}
        >
          {dict.keysAdvanced.editor.change}
        </Button>
      </span>
    </div>
  );
}

/** The advanced-key editor dialog. Mount it to open it. */
export function AdvancedEditor({ state, request, confirmedKeys = [], onSave, onClose }: AdvancedEditorProps) {
  const dict = useDict();
  const lang = useLang();
  const format = useFormat();
  const t = dict.keysAdvanced;
  const idBase = useId();
  const [def, setDef] = useState<AdvancedDefView>(() => initialDef(request));
  const [target, setTarget] = useState<TargetValue>(() => initialTarget(request, confirmedKeys));
  // A new definition follows the chosen key (its own usage) until it is changed by hand.
  const [touched, setTouched] = useState(request.mode === "edit" || request.item !== null);
  const [picking, setPicking] = useState<{ target: PickTarget; opener: string } | null>(null);
  const pendingFocus = useRef<string | null>(null);

  useEffect(() => {
    const id = pendingFocus.current;
    if (id === null || picking !== null) return;
    pendingFocus.current = null;
    document.getElementById(id)?.focus();
  });

  const change = (next: AdvancedDefView) => {
    setDef(next);
    setTouched(true);
  };

  const changeTarget = (next: TargetValue) => {
    setTarget(next);
    if (!touched && next.key !== null && next.key !== target.key && request.mode === "create") {
      setDef(newDef(request.type, ownUsage(next.key)));
    }
  };

  const pick = (pickTarget: PickTarget, opener: string) => {
    setPicking({ target: pickTarget, opener });
  };

  const back = () => {
    if (picking !== null) pendingFocus.current = picking.opener;
    setPicking(null);
  };

  const picked = (usage: number) => {
    if (picking === null) return;
    const what = picking.target;
    if (def.type === "dks" && what.what === "row") {
      const rows = fourRows(def.rows);
      if (what.position === null) {
        const free = rows.findIndex((row) => row === null);
        if (free >= 0) {
          rows[free] = { key: usage, steps: ["tap", "none", "none", "none"] };
          // The focus goes to the new row's key.
          pendingFocus.current = cellId(idBase, free, 0);
        }
      } else {
        const row = rows[what.position];
        if (row !== null && row !== undefined) rows[what.position] = { ...row, key: usage };
        pendingFocus.current = picking.opener;
      }
      change({ ...def, rows });
    } else {
      if (def.type === "mod_tap" && what.what === "tap") change({ ...def, tap: usage });
      if (def.type === "mod_tap" && what.what === "hold") change({ ...def, hold: usage });
      if (def.type === "toggle" && what.what === "toggle") change({ ...def, key: usage });
      pendingFocus.current = picking.opener;
    }
    setPicking(null);
  };

  const pickWhat = (pickTarget: PickTarget): string => {
    switch (pickTarget.what) {
      case "row": {
        const position =
          pickTarget.position ?? (def.type === "dks" ? fourRows(def.rows).findIndex((row) => row === null) : 0);
        return t.dks.rowKeyWhat({ n: position + 1 });
      }
      case "tap":
        return t.mt.tap;
      case "hold":
        return t.mt.hold;
      case "toggle":
        return t.toggle.key;
    }
  };

  const editing = request.mode === "edit";
  const slotView = editing ? state.advanced.find((slot) => slot.slot === request.slot) : undefined;
  const typeName = dict.keys.advancedTypes[def.type];
  const title =
    picking !== null
      ? t.editor.pickTitle({ what: pickWhat(picking.target) })
      : editing
        ? t.editor.titleEdit({ name: dict.keys.action.advancedTyped({ n: request.slot + 1, type: typeName }) })
        : t.editor.titleNew({ type: typeName });

  const pickedUsage = (pickTarget: PickTarget): number | null => {
    if (def.type === "dks" && pickTarget.what === "row" && pickTarget.position !== null) {
      return fourRows(def.rows)[pickTarget.position]?.key ?? null;
    }
    if (def.type === "mod_tap" && pickTarget.what === "tap") return def.tap;
    if (def.type === "mod_tap" && pickTarget.what === "hold") return def.hold;
    if (def.type === "toggle" && pickTarget.what === "toggle") return def.key;
    return null;
  };

  const reason =
    request.mode === "create" && target.key === null
      ? t.editor.needTarget
      : request.mode === "create" && needsConfirmation(state, target)
        ? t.editor.needConfirm
        : defProblem(def, state, dict);

  const save = () => {
    if (reason !== null) return;
    const key = target.key;
    const critical = key !== null && target.layer === "fn" && keyAt(state, key)?.builtin?.critical === true;
    onSave({
      def,
      target: request.mode === "create" && key !== null ? { layer: target.layer, key } : null,
      ack: critical && target.confirmed ? { key, critical: true } : null,
    });
  };

  const current = picking === null ? null : pickedUsage(picking.target);

  let editor: ReactNode;
  switch (def.type) {
    case "dks":
      editor = (
        <DksEditor
          def={def}
          state={state}
          idBase={idBase}
          usage={target.key === null ? null : ownUsage(target.key)}
          onChange={change}
          onPick={pick}
        />
      );
      break;
    case "mod_tap": {
      const confirmedMs = ACTION_CATALOG.mod_tap.confirmed_ms[0];
      const { mt_hold_ms_min: min, mt_hold_ms_max: max, mt_hold_ms_step: step } = state.limits;
      editor = (
        <div className="keys-adv-mt ow-stack">
          <KeyField
            id={`${idBase}-tap`}
            label={t.mt.tap}
            usage={def.tap}
            onChange={() => {
              pick({ what: "tap" }, `${idBase}-tap`);
            }}
          />
          <KeyField
            id={`${idBase}-hold`}
            label={t.mt.hold}
            usage={def.hold}
            onChange={() => {
              pick({ what: "hold" }, `${idBase}-hold`);
            }}
          />
          <Slider
            label={
              <span className="keys-adv-label">
                <span>{t.mt.holdTime}</span>
                <EvidenceBadge evidence={holdTimeEvidence(def.hold_ms)} size="sm" />
              </span>
            }
            value={def.hold_ms}
            min={min}
            max={max}
            step={step}
            bigStep={step * 10}
            valueText={(value) => t.mt.ms({ n: value })}
            field={<span className="ow-num keys-adv-value">{t.mt.ms({ n: def.hold_ms })}</span>}
            hint={t.mt.holdTimeHint({ min: t.mt.ms({ n: min }), max: t.mt.ms({ n: max }) })}
            onChange={(value) => {
              change({ ...def, hold_ms: value });
            }}
          />
          {confirmedMs === undefined || def.hold_ms === confirmedMs ? null : (
            <div>
              <Button
                size="sm"
                onClick={() => {
                  change({ ...def, hold_ms: confirmedMs });
                }}
              >
                {t.mt.useConfirmed({ ms: t.mt.ms({ n: confirmedMs }) })}
              </Button>
            </div>
          )}
        </div>
      );
      break;
    }
    case "toggle":
      editor = (
        <div className="keys-adv-tgl ow-stack">
          <KeyField
            id={`${idBase}-toggle`}
            label={t.toggle.key}
            usage={def.key}
            onChange={() => {
              pick({ what: "toggle" }, `${idBase}-toggle`);
            }}
          />
        </div>
      );
      break;
  }

  return (
    <Dialog
      open
      size="lg"
      title={title}
      className="keys-adv-editor"
      onClose={picking === null ? onClose : back}
      actions={
        picking !== null ? (
          <Button onClick={back}>{dict.common.actions.back}</Button>
        ) : (
          <>
            <Button onClick={onClose}>{dict.common.actions.cancel}</Button>
            <Button variant="primary" disabledReason={reason} onClick={save}>
              {editing || request.item !== null ? t.editor.update : t.editor.add}
            </Button>
          </>
        )
      }
    >
      {picking !== null ? (
        <ActionPicker
          layer="base"
          tabs={["keys"]}
          value={current === null ? null : { kind: "key", usage: current }}
          onPick={(value) => {
            if (value.kind === "key") picked(value.usage);
          }}
        />
      ) : (
        <div className="keys-adv-editor__body ow-stack">
          {request.mode === "create" ? (
            <TargetKey state={state} value={target} onChange={changeTarget} />
          ) : (
            <p className="ow-small">
              {slotView === undefined || slotView.bindings.length === 0
                ? dict.keys.slot.orphan
                : dict.keys.slot.usedBy({ keys: bindingsText(slotView.bindings, dict) })}
            </p>
          )}
          {editor}
          <p className="keys-adv-sentence" aria-live="polite">
            {defSentence(def, dict, lang, (items) => format.list(items))}
          </p>
        </div>
      )}
    </Dialog>
  );
}

/** Props of {@link DksEditor}. */
interface DksEditorProps {
  def: DksDef;
  state: KeysState;
  idBase: string;
  /** The usage of the key it goes on (quick starts fill it in). */
  usage: number | null;
  onChange: (def: AdvancedDefView) => void;
  onPick: (target: PickTarget, opener: string) => void;
}

/** The dynamic-keystroke editor. */
function DksEditor({ def, state, idBase, usage, onChange, onPick }: DksEditorProps) {
  const dict = useDict();
  const lang = useLang();
  const t = dict.keysAdvanced.dks;
  const max = state.limits.dks_first_point_max ?? 30;
  const mm = (tenths: number) => tenthsMm(tenths, lang, dict);
  const rows = fourRows(def.rows);

  const setRows = (next: (DksRowView | null)[]) => {
    onChange({ ...def, rows: next });
  };

  const onStep = (position: number, stage: number, step: DksStepView) => {
    const row = rows[position];
    if (row === null || row === undefined) return;
    const next = [...rows];
    next[position] = { ...row, steps: setStep(row.steps, stage, step) };
    setRows(next);
  };

  return (
    <div className="keys-adv-dks-editor ow-stack">
      <div className="keys-adv-point">
        <Slider
          label={t.firstPoint}
          value={Math.min(Math.max(def.first_point, 1), max)}
          min={1}
          max={max}
          step={1}
          valueText={mm}
          field={<span className="ow-num keys-adv-value">{mm(def.first_point)}</span>}
          hint={t.firstPointHint({ max: mm(max) })}
          onChange={(value) => {
            onChange({ ...def, first_point: value });
          }}
        />
        <TravelGauge
          label={t.gauge}
          showLegend
          orientation="horizontal"
          length={260}
          formatMm={(value) => mm(Math.round(value * 10))}
          points={[
            { mm: def.first_point / 10, label: t.gaugeFirst },
            { mm: max / 10, label: t.gaugeBottom },
          ]}
        />
      </div>
      <div className="keys-adv-quick">
        <p className="ow-field__label">{t.quickTitle}</p>
        <p className="ow-small ow-muted">{t.quickHint}</p>
        <div className="keys-adv-quick__list">
          {QUICK_STARTS.map((quick) => (
            <span key={quick.id} className="keys-adv-quick__item">
              <Button
                size="sm"
                onClick={() => {
                  setRows(applyQuickStart(quick, rows, usage));
                }}
              >
                {t.quick[quick.id]}
              </Button>
              <EvidenceBadge evidence={quickStartEvidence(quick)} size="sm" />
            </span>
          ))}
        </div>
      </div>
      <DksGrid
        rows={rows}
        idBase={idBase}
        onStep={onStep}
        onPickKey={(position, opener) => {
          if (position === null && rows.every((row) => row !== null)) return;
          onPick({ what: "row", position }, opener);
        }}
        onRemove={(position) => {
          const next = [...rows];
          next[position] = null;
          setRows(next);
        }}
      />
    </div>
  );
}
