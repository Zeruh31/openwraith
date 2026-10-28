// A key's travel from the top (0 mm, not pressed) to the bottom (3.40 mm on
// the W75): the actuation point, Rapid Trigger bands, the dead zones, DKS
// points, and the live depth with the deepest point so far. The drawing is
// decorative; the same facts are a text list (visually hidden unless
// `showLegend`), so nothing is shown by position or colour alone. The gauge
// never announces changes itself (a live view decides when to speak).

import { useId } from "react";

import { useDict, type Dict } from "../i18n";

/** The W75's full key travel in millimetres. */
export const W75_TRAVEL_MM = 3.4;

/** A labelled point on the travel (e.g. a DKS point). */
export interface GaugePoint {
  mm: number;
  /** Its name ("Press", "Bottom"). */
  label: string;
}

/** Props of {@link TravelGauge}. */
export interface TravelGaugeProps {
  /** The accessible name ("W: key travel"). */
  label?: string | undefined;
  /** Formats a distance ("1,20 mm"). */
  formatMm: (mm: number) => string;
  /** The full travel (default 3.40 mm). */
  maxMm?: number | undefined;
  actuationMm?: number | null | undefined;
  /** Rapid Trigger: distance down that presses again, distance up that releases. */
  rt?: { pressMm: number; releaseMm: number } | null | undefined;
  /** Dead zones at the top and the bottom. */
  deadZones?: { topMm: number; bottomMm: number } | null | undefined;
  /** Labelled points (DKS). */
  points?: readonly GaugePoint[] | undefined;
  /** The live depth. */
  depthMm?: number | null | undefined;
  /** The deepest point so far. */
  peakMm?: number | null | undefined;
  orientation?: "vertical" | "horizontal" | undefined;
  /** Length of the travel axis in pixels (default 160). */
  length?: number | undefined;
  /** Show the text list under the drawing (default: only for screen readers). */
  showLegend?: boolean | undefined;
}

function clampMm(mm: number, max: number): number {
  return Math.min(max, Math.max(0, mm));
}

/** The text alternative of a gauge, one line per fact (tests and other views use it too). */
export function gaugeLines(props: TravelGaugeProps, dict: Dict): string[] {
  const g = dict.common.pickers.gauge;
  const max = props.maxMm ?? W75_TRAVEL_MM;
  const f = props.formatMm;
  const lines: string[] = [g.top];
  const dead = props.deadZones;
  if (dead !== null && dead !== undefined) {
    if (dead.topMm > 0) lines.push(g.deadTop({ value: f(dead.topMm) }));
    if (dead.bottomMm > 0) lines.push(g.deadBottom({ value: f(dead.bottomMm) }));
  }
  if (props.actuationMm !== null && props.actuationMm !== undefined) lines.push(g.actuation({ value: f(props.actuationMm) }));
  if (props.rt !== null && props.rt !== undefined) {
    lines.push(g.rtPress({ value: f(props.rt.pressMm) }), g.rtRelease({ value: f(props.rt.releaseMm) }));
  }
  for (const point of props.points ?? []) lines.push(g.point({ label: point.label, value: f(point.mm) }));
  if (props.depthMm !== null && props.depthMm !== undefined) lines.push(g.depth({ value: f(props.depthMm) }));
  if (props.peakMm !== null && props.peakMm !== undefined) lines.push(g.peak({ value: f(props.peakMm) }));
  lines.push(g.bottom({ value: f(max) }));
  return lines;
}

/** The travel gauge. */
export function TravelGauge(props: TravelGaugeProps) {
  const dict = useDict();
  const id = useId();
  const {
    label,
    maxMm = W75_TRAVEL_MM,
    actuationMm,
    rt,
    deadZones,
    points = [],
    depthMm,
    peakMm,
    orientation = "vertical",
    length = 160,
    showLegend = false,
  } = props;
  const vertical = orientation === "vertical";
  const thickness = 28;
  const pad = 6;
  const at = (mm: number) => pad + (clampMm(mm, maxMm) / maxMm) * length;
  // Rectangles along the travel axis: [from, to] in mm.
  const band = (from: number, to: number, className: string, key: string) => {
    const a = at(Math.min(from, to));
    const b = at(Math.max(from, to));
    return vertical ? (
      <rect key={key} className={className} x={0} y={a} width={thickness} height={Math.max(1, b - a)} rx={3} />
    ) : (
      <rect key={key} className={className} x={a} y={0} width={Math.max(1, b - a)} height={thickness} rx={3} />
    );
  };
  const line = (mm: number, className: string, key: string, extra = 6) => {
    const p = at(mm);
    return vertical ? (
      <line key={key} className={className} x1={-extra} x2={thickness + extra} y1={p} y2={p} />
    ) : (
      <line key={key} className={className} y1={-extra} y2={thickness + extra} x1={p} x2={p} />
    );
  };
  const width = vertical ? thickness : length + pad * 2;
  const height = vertical ? length + pad * 2 : thickness;
  const lines = gaugeLines(props, dict);
  const listId = `${id}-text`;

  return (
    <figure
      className={`ow-gauge ow-gauge--${orientation}`}
      role="group"
      aria-label={label ?? dict.common.pickers.gauge.label}
      aria-describedby={listId}
    >
      <svg
        className="ow-gauge__svg"
        width={width + 24}
        height={height + 2}
        viewBox={`-12 -1 ${width + 24} ${height + 2}`}
        aria-hidden="true"
        focusable="false"
      >
        {band(0, maxMm, "ow-gauge__track", "track")}
        {deadZones !== null && deadZones !== undefined && deadZones.topMm > 0
          ? band(0, deadZones.topMm, "ow-gauge__dead", "dead-top")
          : null}
        {deadZones !== null && deadZones !== undefined && deadZones.bottomMm > 0
          ? band(maxMm - deadZones.bottomMm, maxMm, "ow-gauge__dead", "dead-bottom")
          : null}
        {depthMm !== null && depthMm !== undefined ? band(0, depthMm, "ow-gauge__depth", "depth") : null}
        {rt !== null && rt !== undefined && actuationMm !== null && actuationMm !== undefined
          ? band(actuationMm - rt.releaseMm, actuationMm + rt.pressMm, "ow-gauge__rt", "rt")
          : null}
        {actuationMm !== null && actuationMm !== undefined ? line(actuationMm, "ow-gauge__actuation", "actuation") : null}
        {points.map((point, index) => {
          const p = at(point.mm);
          return vertical ? (
            <circle key={`p${index}`} className="ow-gauge__point" cx={thickness / 2} cy={p} r={4} />
          ) : (
            <circle key={`p${index}`} className="ow-gauge__point" cx={p} cy={thickness / 2} r={4} />
          );
        })}
        {peakMm !== null && peakMm !== undefined ? line(peakMm, "ow-gauge__peak", "peak", 2) : null}
      </svg>
      <ul id={listId} className={showLegend ? "ow-gauge__legend ow-small" : "sr-only"}>
        {lines.map((text, index) => (
          <li key={index}>{text}</li>
        ))}
      </ul>
    </figure>
  );
}
