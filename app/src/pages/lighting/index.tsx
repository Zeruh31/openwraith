// The Lighting page: the main light's effect, colour, brightness and speed.
//
// The controls only change a local draft (no IPC while dragging or typing).
// "Review and write…" opens the shared confirmation flow: the backend plans
// the edit (read-only), the user reviews the plan's current → new values and
// presses Write, the backend saves a safety backup, writes, verifies, waits
// for the keyboard to save and reads the light again; the report offers Undo.
// Side light and per-key colours are not edited in this version.

import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { ApplyBar } from "../../components/ApplyBar";
import { Button } from "../../components/Button";
import { Callout, EmptyState, Spinner, UnverifiedMarker } from "../../components/Feedback";
import { ColorField, Slider } from "../../components/Fields";
import { Card, ColorSwatch, PageHeader, ProfileBadge } from "../../components/Layout";
import { EditReportView } from "../../flow/Report";
import { WriteFlow } from "../../flow/WriteFlow";
import { getDict, useDict } from "../../i18n";
import { useFormat } from "../../i18n/format";
import type { EditReportView as EditReport, LightEditIntent, LightEditPreview } from "../../ipc/bindings";
import { api } from "../../ipc/commands";
import { previewUi } from "../../preview/ui";
import { navigate } from "../../routes";
import { readAgain } from "../../shell/actions";
import { connectionSummary } from "../../shell/statusText";
import { isLocked, useOperationLock } from "../../state/operation";
import { useSnapshot } from "../../state/snapshot";
import { useStatus } from "../../state/status";
import { toast } from "../../state/toasts";
import { basisOf, clearDraft, rebase, seedDraft, setDraft, useDraftState } from "./draftStore";
import { EffectTiles } from "./EffectTiles";
import { applyBlockText, modeName } from "./labels";
import { LightPreview } from "./LightPreview";
import { LightReview } from "./LightReview";
import {
  applyBlock,
  buildIntent,
  controlsFor,
  draftFromPreview,
  EMPTY_DRAFT,
  hasDraftFields,
  intentChanges,
  offeredModes,
  QUICK_COLORS,
  shownLight,
  withField,
  type DraftField,
  type LightChange,
  type LightingPreviewUi,
} from "./model";
import "./lighting.css";

/** The open confirmation flow: the intent it plans and the keyboard's maxima. */
interface FlowState {
  intent: LightEditIntent;
  maxBrightness: number;
  maxSpeed: number;
}

/** The Lighting page. */
export default function LightingPage() {
  const dict = useDict();
  const format = useFormat();
  const status = useStatus();
  const { snapshot, stale, loading } = useSnapshot();
  const lock = useOperationLock();
  const stored = useDraftState();
  const hintId = useId();
  const [flow, setFlow] = useState<FlowState | null>(null);
  const flowOpen = useRef(false);
  const latestIntent = useRef<LightEditIntent | null>(null);
  const t = dict.lighting;

  const basis = basisOf(snapshot);
  const light = snapshot?.light ?? null;
  const current = light?.current ?? null;
  // A draft made on another profile or connection doesn't apply (it is discarded below).
  const draft = basis === null || stored.basis === null || stored.basis === basis ? stored.draft : EMPTY_DRAFT;
  const shown = light === null || current === null ? null : shownLight(light, current, draft);
  const intent = light === null || snapshot === null ? null : buildIntent(light, draft, snapshot.revision);
  const dirty = intent !== null;

  // Browser-preview scenarios may preset a draft (never outside `vite --mode mock`).
  useEffect(() => {
    seedDraft(draftFromPreview(previewUi<LightingPreviewUi>("lighting")));
  }, []);

  // The keyboard is on another profile, or was connected again: the draft
  // no longer applies. While a write flow is open the change comes from (or
  // is explained by) that flow, so there is no toast then.
  useEffect(() => {
    if (basis !== null && rebase(basis) && !flowOpen.current) {
      const words = getDict().lighting.discarded;
      toast({ kind: "info", title: words.title, body: words.body });
    }
  }, [basis]);

  useEffect(() => {
    latestIntent.current = intent;
  });

  const edit = (field: DraftField, value: number | string) => {
    if (current === null) return;
    setDraft(basis, (previous) => withField(previous, current, field, value));
  };

  const openFlow = () => {
    if (flowOpen.current || intent === null || light === null || isLocked()) return;
    flowOpen.current = true;
    setFlow({ intent, maxBrightness: light.max_brightness, maxSpeed: light.max_speed });
  };

  const onFlowClose = (report: EditReport | null) => {
    flowOpen.current = false;
    setFlow(null);
    if (report?.anything_written === true) clearDraft();
  };

  const ready = status.connection.state === "ready";

  // The flow stays mounted whatever happens to the snapshot: a lost
  // connection during the write clears it, and the progress and then the
  // report (MAY BE LOST, the safety backup, Undo) must stay in view. It sits
  // at the same place in both renders below, so React keeps it.
  const flowElement =
    flow === null ? null : (
      <WriteFlow<LightEditPreview, EditReport>
        kind="light_edit"
        prepare={() => api.prepareLightEdit(latestIntent.current ?? flow.intent)}
        token={(preview) => preview.token}
        title={(preview) => t.review.title({ n: preview.profile })}
        review={(preview) => (
          <LightReview preview={preview} maxBrightness={flow.maxBrightness} maxSpeed={flow.maxSpeed} />
        )}
        confirmLabel={() => dict.common.actions.writeToKeyboard}
        writesSettings
        apply={(token, onProgress) => api.applyLightEdit(token, onProgress)}
        result={(report) => <EditReportView report={report} />}
        onClose={onFlowClose}
      />
    );

  if (snapshot === null || light === null) {
    const waiting = loading || ready;
    return (
      <div className="li-page">
        <>
          <PageHeader title={t.title} description={t.description} />
          {waiting ? (
            <p className="ow-row">
              <Spinner />
              <span>{t.empty.loading}</span>
            </p>
          ) : (
            <EmptyState
              icon="plug"
              title={t.empty.title}
              action={
                <Button
                  onClick={() => {
                    navigate("device");
                  }}
                >
                  {t.empty.goToKeyboard}
                </Button>
              }
            >
              <p>{connectionSummary(status, dict, format).text}</p>
            </EmptyState>
          )}
        </>
        {flowElement}
      </div>
    );
  }

  const readOnly = stale || !light.editable || current === null;
  const controlsDisabled = readOnly || shown === null || shown.needsEffect;
  const block = applyBlock(status, lock.busy, stale, light, shown);

  const changeText = (change: LightChange): ReactNode => {
    switch (change.field) {
      case "mode":
        return t.applyBar.effect({ from: modeName(dict, change.from), to: modeName(dict, change.to) });
      case "color":
        // Custom shows the stored per-key colours, not the record's colour.
        return (
          <>
            {t.applyBar.color}:{" "}
            {current !== null && !controlsFor(current.mode.key).color ? t.review.perKey : <ColorSwatch color={change.from} />}{" "}
            → <ColorSwatch color={change.to} />
          </>
        );
      case "brightness":
        return t.applyBar.brightness({ from: change.from, to: change.to });
      case "speed":
        return t.applyBar.speed({ from: change.from, to: change.to });
    }
  };
  const changes = intent === null || current === null ? [] : intentChanges(light, current, intent);
  // A draft whose fields the chosen effect doesn't use changes nothing, but
  // it is still a draft (nav dot, profile-switch warning), so Discard works.
  const summary =
    changes.length === 0 ? (
      <span className="ow-muted">{t.applyBar.noChanges}</span>
    ) : (
      <span className="li-summary">
        <span className="ow-strong">{t.applyBar.notWritten}</span>
        {changes.map((change) => (
          <span key={change.field} className="li-summary__part">
            {changeText(change)}
          </span>
        ))}
      </span>
    );

  return (
    <div className="li-page">
      <>
        <PageHeader
          title={t.title}
          description={t.description}
          actions={
            <div className="ow-row">
              <span className="ow-muted ow-small ow-num">{t.readAt({ time: format.time(snapshot.read_at) })}</span>
              <Button size="sm" icon="refresh" disabled={lock.busy || !ready} onClick={readAgain}>
                {dict.common.actions.readAgain}
              </Button>
            </div>
          }
        />

        <Callout kind="info" title={t.perProfile.title}>
          <p>{t.perProfile.body({ n: snapshot.profile })}</p>
        </Callout>

        {light.editable && current !== null ? null : (
          <Callout kind="warning" title={t.disabled.title}>
            <p>{t.disabled.reasons[light.disabled ?? "not_read"]}</p>
          </Callout>
        )}

        {current === null || current.power_on ? null : (
          <Callout kind="warning" title={t.powerOff.title}>
            <p>
              {t.powerOff.body} <UnverifiedMarker />
            </p>
          </Callout>
        )}

        {current === null || shown === null ? null : (
          <div className="li-grid">
            <Card title={t.effects.title} className="li-effects">
              <EffectTiles
                modes={offeredModes(light)}
                current={current.mode}
                selected={shown.mode.id}
                disabled={readOnly}
                describedBy={shown.needsEffect ? hintId : undefined}
                onSelect={(id) => {
                  edit("mode", id);
                }}
              />
              {shown.needsEffect ? (
                <p id={hintId} className="li-hint">
                  {t.effects.unknownHint}
                </p>
              ) : null}
            </Card>

            <Card title={t.controls.title} className="li-controls" actions={<ProfileBadge profile={snapshot.profile} />}>
              <div className="li-previews">
                <LightPreview
                  caption={t.preview.now}
                  mode={current.mode}
                  color={current.color}
                  brightness={current.brightness}
                  maxBrightness={light.max_brightness}
                  perKey={current.mode.key === "custom"}
                />
                {dirty ? (
                  <LightPreview
                    caption={t.preview.after}
                    mode={shown.mode}
                    color={shown.controls.color ? shown.color : current.color}
                    brightness={shown.brightness}
                    maxBrightness={light.max_brightness}
                    perKey={shown.mode.key === "custom"}
                  />
                ) : null}
                <p className="ow-muted ow-small">{t.preview.note}</p>
              </div>

              <div className="li-fields">
                {shown.controls.color ? (
                  <div>
                    <ColorField
                      label={t.color.label}
                      value={shown.color}
                      swatches={QUICK_COLORS.map((c) => ({ hex: c.hex, name: t.color.names[c.name] }))}
                      disabled={controlsDisabled}
                      onChange={(hex) => {
                        edit("color", hex);
                      }}
                    />
                    {shown.controls.fullColor && current.full_color ? (
                      <p className="li-note ow-small">
                        {t.color.fullColorNote} <UnverifiedMarker />
                      </p>
                    ) : null}
                  </div>
                ) : (
                  <div className="li-off">
                    <span className="ow-field__label">{t.color.label}</span>
                    <p className="ow-muted ow-small">{t.color.customNote}</p>
                  </div>
                )}

                <Slider
                  label={t.brightness.label}
                  value={shown.brightness}
                  min={0}
                  max={light.max_brightness}
                  ticks
                  disabled={controlsDisabled}
                  valueText={(value) => t.level({ value, max: light.max_brightness })}
                  field={<span className="li-level ow-num">{t.level({ value: shown.brightness, max: light.max_brightness })}</span>}
                  hint={shown.brightness === 0 ? t.brightness.zeroHint : undefined}
                  onChange={(value) => {
                    edit("brightness", value);
                  }}
                />

                {shown.controls.speed ? (
                  <Slider
                    label={t.speed.label}
                    value={shown.speed}
                    min={0}
                    max={light.max_speed}
                    ticks
                    disabled={controlsDisabled}
                    valueText={(value) => t.level({ value, max: light.max_speed })}
                    field={<span className="li-level ow-num">{t.level({ value: shown.speed, max: light.max_speed })}</span>}
                    hint={
                      <>
                        <UnverifiedMarker /> {t.speed.hint}
                      </>
                    }
                    onChange={(value) => {
                      edit("speed", value);
                    }}
                  />
                ) : (
                  <div className="li-off">
                    <span className="ow-field__label">{t.speed.label}</span>
                    <p className="ow-muted ow-small">{t.speed.notUsed}</p>
                  </div>
                )}
              </div>
            </Card>
          </div>
        )}

        <ApplyBar
          summary={summary}
          dirty={dirty}
          canDiscard={hasDraftFields(draft)}
          onDiscard={clearDraft}
          onApply={openFlow}
          disabledReason={applyBlockText(dict, block)}
        />
      </>
      {flowElement}
    </div>
  );
}
