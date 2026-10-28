// The steps of a macro, in two views over the same list:
// - simple: a tap (a key going down and up) is one item; lone presses and
//   releases show as "down" / "up"; the waits are only shown;
// - detailed: every step with its key, press or release, and the wait after
//   it (the last step never waits).
// Steps move with Alt + Up/Down or the buttons (StepList), are added with a
// key chosen from the catalogue's keys, and can all be removed at once.

import { useId, useMemo, useState } from "react";

import { ACTION_CATALOG, entryName, usageName } from "../../../components/actions";
import { Button } from "../../../components/Button";
import { SegmentedControl } from "../../../components/Controls";
import { StepList } from "../../../components/StepList";
import { useDict, type Dict } from "../../../i18n";
import type { CatalogGroup, KeyMotionView, MacroStepView } from "../../../ipc/bindings";
import type { StepsView } from "./draftStore";
import {
  addTap,
  appendSteps,
  DEFAULT_DELAY_MS,
  moveItem,
  moveStep,
  removeItem,
  removeStep,
  simpleItems,
  stepKey,
  updateStep,
  validDelay,
  type MacroLimits,
  type SimpleItem,
} from "./model";

interface KeyGroup {
  group: CatalogGroup;
  keys: { usage: number; id: string; name: (dict: Dict) => string }[];
}

/** The keys a macro step can press, grouped as the catalogue groups them. */
const KEY_GROUPS: readonly KeyGroup[] = (() => {
  const groups: KeyGroup[] = [];
  for (const entry of ACTION_CATALOG.entries) {
    if (entry.tab !== "keys" || entry.value.kind !== "key") continue;
    const usage = entry.value.usage;
    let group = groups.find((item) => item.group === entry.group);
    if (group === undefined) {
      group = { group: entry.group, keys: [] };
      groups.push(group);
    }
    group.keys.push({ usage, id: entry.id, name: (dict) => entryName(entry, dict) });
  }
  return groups;
})();

/** Whether the catalogue offers a usage as a macro step. */
export function isStepKey(usage: number): boolean {
  return KEY_GROUPS.some((group) => group.keys.some((key) => key.usage === usage));
}

/** A simple-view item in words ("Tap A", "Left Shift down"). */
export function itemText(item: SimpleItem, dict: Dict): string {
  const t = dict.keysMacros.editor.item;
  const key = usageName(item.usage, dict);
  return item.kind === "tap" ? t.tap({ key }) : item.kind === "press" ? t.press({ key }) : t.release({ key });
}

/** A step in words ("A down"). */
export function stepText(step: MacroStepView, dict: Dict): string {
  const t = dict.keysMacros.editor.item;
  const key = usageName(step.usage, dict);
  return step.motion === "press" ? t.press({ key }) : t.release({ key });
}

function KeySelect({
  id,
  value,
  onChange,
  disabled,
  placeholder,
  label,
}: {
  id: string;
  value: number | null;
  onChange: (usage: number | null) => void;
  disabled: boolean;
  placeholder?: string | undefined;
  label?: string | undefined;
}) {
  const dict = useDict();
  return (
    <select
      id={id}
      className="ow-input keys-macros__key-select"
      value={value === null ? "" : String(value)}
      disabled={disabled}
      aria-label={label}
      onChange={(event) => {
        const next = event.currentTarget.value;
        onChange(next === "" ? null : Number(next));
      }}
    >
      {placeholder === undefined ? null : <option value="">{placeholder}</option>}
      {value !== null && !isStepKey(value) ? <option value={value}>{usageName(value, dict)}</option> : null}
      {KEY_GROUPS.map((group) => (
        <optgroup key={group.group} label={dict.keys.groups[group.group]}>
          {group.keys.map((key) => (
            <option key={key.id} value={key.usage}>
              {key.name(dict)}
            </option>
          ))}
        </optgroup>
      ))}
    </select>
  );
}

/** Props of {@link StepsEditor}. */
export interface StepsEditorProps {
  steps: readonly MacroStepView[];
  onSteps: (steps: MacroStepView[]) => void;
  view: StepsView;
  onView: (view: StepsView) => void;
  limits: MacroLimits;
  disabled: boolean;
}

/** The steps of a macro, editable in the simple or the detailed view. */
export function StepsEditor({ steps, onSteps, view, onView, limits, disabled }: StepsEditorProps) {
  const dict = useDict();
  const t = dict.keysMacros.editor;
  const id = useId();
  const [adding, setAdding] = useState<number | null>(null);
  const items = useMemo(() => simpleItems(steps), [steps]);
  const full = steps.length >= limits.maxSteps;
  const addReason = (count: number): string | null =>
    steps.length + count > limits.maxSteps ? t.full({ max: limits.maxSteps }) : adding === null ? t.needKeyToAdd : null;

  const add = (motions: readonly KeyMotionView[]) => {
    if (adding === null) return;
    onSteps(
      motions.length === 2
        ? addTap(steps, adding, DEFAULT_DELAY_MS)
        : appendSteps(steps, motions.map((motion) => ({ usage: adding, motion })), DEFAULT_DELAY_MS),
    );
  };

  return (
    <div className="keys-macros__steps ow-stack">
      <div className="keys-macros__steps-head">
        <SegmentedControl<StepsView>
          label={t.view}
          size="sm"
          options={[
            { value: "simple", label: t.views.simple },
            { value: "detailed", label: t.views.detailed },
          ]}
          value={view}
          onChange={onView}
        />
        <span className="ow-small ow-muted ow-num" aria-live="polite">
          {t.stepCount({ n: steps.length, max: limits.maxSteps })}
        </span>
      </div>
      <p className="ow-small ow-muted">{view === "simple" ? t.simpleHint : t.detailedHint}</p>
      {view === "simple" ? (
        <StepList<SimpleItem>
          label={t.listLabel}
          items={items}
          itemKey={(item) => stepKey(steps[item.start])}
          itemLabel={(item) => itemText(item, dict)}
          renderItem={(item) => (
            <span className="keys-macros__item">
              <span className="ow-strong">{itemText(item, dict)}</span>
              {item.waitMs > 0 && item.start + item.length < steps.length ? (
                <span className="ow-small ow-muted ow-num">{t.item.wait({ ms: item.waitMs })}</span>
              ) : null}
            </span>
          )}
          onMove={(from, to) => {
            onSteps(moveItem(steps, from, to));
          }}
          onRemove={(index) => {
            onSteps(removeItem(steps, index));
          }}
          disabled={disabled}
        />
      ) : (
        <>
          {steps.length === 0 ? null : (
            <div className="keys-macros__step-head ow-muted" aria-hidden="true">
              <span />
              <span className="keys-macros__step">
                <span>{t.columns.key}</span>
                <span>{t.columns.motion}</span>
                <span>{t.columns.delay}</span>
              </span>
              <span className="keys-macros__step-head-actions" />
            </div>
          )}
          <StepList<MacroStepView>
            label={t.listLabel}
            items={steps}
            itemKey={(step) => stepKey(step)}
            itemLabel={(step) => stepText(step, dict)}
            renderItem={(step, index) => {
              const last = index === steps.length - 1;
              const invalid = !last && !validDelay(step.delay_after_ms, limits);
              return (
                <span className="keys-macros__step">
                  <KeySelect
                    id={`${id}-k-${index}`}
                    label={t.stepKey({ n: index + 1 })}
                    value={step.usage}
                    disabled={disabled}
                    onChange={(usage) => {
                      if (usage !== null) onSteps(updateStep(steps, index, { usage }));
                    }}
                  />
                  <select
                    className="ow-input keys-macros__motion"
                    aria-label={t.stepMotion({ n: index + 1 })}
                    value={step.motion}
                    disabled={disabled}
                    onChange={(event) => {
                      const motion = event.currentTarget.value === "release" ? "release" : "press";
                      onSteps(updateStep(steps, index, { motion }));
                    }}
                  >
                    <option value="press">{t.motion.press}</option>
                    <option value="release">{t.motion.release}</option>
                  </select>
                  <input
                    type="number"
                    className="ow-input ow-num keys-macros__delay-input"
                    aria-label={t.stepDelay({ n: index + 1 })}
                    aria-invalid={invalid || undefined}
                    title={last ? t.lastNoWait : undefined}
                    min={0}
                    max={limits.maxDelayMs}
                    step={1}
                    value={Number.isNaN(step.delay_after_ms) ? "" : step.delay_after_ms}
                    disabled={disabled || last}
                    onChange={(event) => {
                      const text = event.currentTarget.value;
                      onSteps(updateStep(steps, index, { delay_after_ms: text === "" ? Number.NaN : Number(text) }));
                    }}
                  />
                </span>
              );
            }}
            onMove={(from, to) => {
              onSteps(moveStep(steps, from, to));
            }}
            onRemove={(index) => {
              onSteps(removeStep(steps, index));
            }}
            disabled={disabled}
          />
        </>
      )}
      <div className="keys-macros__add">
        <div className="keys-macros__field">
          <label htmlFor={`${id}-add`} className="ow-field__label">
            {t.addKey}
          </label>
          <KeySelect id={`${id}-add`} value={adding} onChange={setAdding} disabled={disabled || full} placeholder={t.chooseKey} />
        </div>
        <div className="ow-row keys-macros__add-buttons">
          <Button
            size="sm"
            icon="plus"
            disabled={disabled}
            disabledReason={addReason(2)}
            onClick={() => {
              add(["press", "release"]);
            }}
          >
            {t.addTap}
          </Button>
          {view === "detailed" ? (
            <>
              <Button
                size="sm"
                disabled={disabled || addReason(1) !== null}
                onClick={() => {
                  add(["press"]);
                }}
              >
                {t.addPress}
              </Button>
              <Button
                size="sm"
                disabled={disabled || addReason(1) !== null}
                onClick={() => {
                  add(["release"]);
                }}
              >
                {t.addRelease}
              </Button>
            </>
          ) : null}
        </div>
      </div>
      {steps.length === 0 ? null : (
        <div>
          <Button
            size="sm"
            variant="ghost"
            icon="trash"
            disabled={disabled}
            onClick={() => {
              onSteps([]);
            }}
          >
            {t.clearSteps}
          </Button>
        </div>
      )}
    </div>
  );
}
