// The confirmation body of a light edit: the plan's current and target light
// side by side (from the backend's read-only plan, not the page's draft),
// the effect-change note and the packet count under technical details.

import type { ReactNode } from "react";

import { Callout, UnverifiedMarker } from "../../components/Feedback";
import { ColorSwatch, Table, TechnicalDetails, type TableColumn } from "../../components/Layout";
import { useDict } from "../../i18n";
import type { LightEditPreview, LightValue } from "../../ipc/bindings";
import { modeName } from "./labels";
import { controlsFor } from "./model";

interface Row {
  key: "mode" | "color" | "brightness" | "speed";
  label: string;
  now: ReactNode;
  next: ReactNode;
  changed: boolean;
}

/** Props of {@link LightReview}. */
export interface LightReviewProps {
  preview: LightEditPreview;
  /** The keyboard's maxima (from the snapshot). */
  maxBrightness: number;
  maxSpeed: number;
}

/** What a light edit will write. */
export function LightReview({ preview, maxBrightness, maxSpeed }: LightReviewProps) {
  const dict = useDict();
  const t = dict.lighting.review;
  const { current, target } = preview;

  const colorCell = (value: LightValue): { text: string; node: ReactNode } =>
    controlsFor(value.mode.key).color
      ? { text: value.color, node: <ColorSwatch color={value.color} size="md" /> }
      : { text: t.perKey, node: t.perKey };
  const speedCell = (value: LightValue): string =>
    controlsFor(value.mode.key).speed ? dict.lighting.level({ value: value.speed, max: maxSpeed }) : t.notUsed;
  const brightnessCell = (value: LightValue): string =>
    dict.lighting.level({ value: value.brightness, max: maxBrightness });

  const nowColor = colorCell(current);
  const newColor = colorCell(target);
  const rows: Row[] = [
    {
      key: "mode",
      label: dict.lighting.effects.title,
      now: modeName(dict, current.mode),
      next: modeName(dict, target.mode),
      changed: current.mode.id !== target.mode.id,
    },
    { key: "color", label: dict.lighting.color.label, now: nowColor.node, next: newColor.node, changed: nowColor.text !== newColor.text },
    {
      key: "brightness",
      label: dict.lighting.brightness.label,
      now: brightnessCell(current),
      next: brightnessCell(target),
      changed: current.brightness !== target.brightness,
    },
    {
      key: "speed",
      label: dict.lighting.speed.label,
      now: speedCell(current),
      next: speedCell(target),
      changed: speedCell(current) !== speedCell(target),
    },
  ];

  const columns: readonly TableColumn<Row>[] = [
    {
      key: "setting",
      header: t.setting,
      rowHeader: true,
      render: (row) => (
        <>
          {row.label}
          {row.changed ? <span className="sr-only"> {t.changes}</span> : null}
        </>
      ),
    },
    { key: "now", header: t.now, render: (row) => row.now },
    {
      key: "new",
      header: t.new,
      render: (row) => <span className={row.changed ? "li-review__new" : "ow-muted"}>{row.next}</span>,
    },
  ];

  const fullColorShown =
    target.full_color && controlsFor(target.mode.key).fullColor && target.color !== current.color;

  return (
    <div className="li-review ow-stack">
      <p>{t.intro({ n: preview.profile })}</p>
      <Table
        columns={columns}
        rows={rows}
        rowKey={(row) => row.key}
        rowClass={(row) => (row.changed ? "li-review__row--changed" : undefined)}
      />
      {preview.changes_mode ? <Callout kind="info">{t.modeChange}</Callout> : null}
      {fullColorShown ? (
        <Callout kind="info">
          <p>
            {dict.lighting.color.fullColorNote} <UnverifiedMarker />
          </p>
        </Callout>
      ) : null}
      {target.power_on ? null : (
        <Callout kind="warning" title={dict.lighting.powerOff.title}>
          <p>
            {dict.lighting.powerOff.body} <UnverifiedMarker />
          </p>
        </Callout>
      )}
      <TechnicalDetails>{preview.changes_mode ? t.packetsModeChange : t.packets({ n: preview.packets })}</TechnicalDetails>
    </div>
  );
}
