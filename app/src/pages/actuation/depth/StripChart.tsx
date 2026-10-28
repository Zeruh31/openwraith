// The depth of one key over the last seconds, as a line (depth grows
// downwards, like the key's travel), with its actuation point when "Show
// where keys trigger" is on. Decorative: the same facts are in the tab's
// text (the key's value, the gauge's text and the hidden table).

import type { DepthPoint } from "./model";
import { chartPoints } from "./model";

const WIDTH = 320;
const HEIGHT = 96;

/** Props of {@link StripChart}. */
export interface StripChartProps {
  points: readonly DepthPoint[];
  /** `open_ms` of the latest frame (the right edge). */
  at: number;
  /** The full travel, in trigger units. */
  travel: number;
  /** The actuation point to draw, in trigger units. */
  actuation: number | null;
}

/** The strip chart. */
export function StripChart({ points, at, travel, actuation }: StripChartProps) {
  const line = chartPoints(points, at, travel, WIDTH, HEIGHT);
  const y = actuation === null ? null : Math.round((Math.min(travel, Math.max(0, actuation)) / travel) * HEIGHT * 10) / 10;
  return (
    <svg
      className="act-depth-chart"
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      width={WIDTH}
      height={HEIGHT}
      preserveAspectRatio="none"
      aria-hidden="true"
      focusable="false"
    >
      <rect className="act-depth-chart__frame" x={0} y={0} width={WIDTH} height={HEIGHT} />
      {y === null ? null : <line className="act-depth-chart__actuation" x1={0} x2={WIDTH} y1={y} y2={y} />}
      {line === "" ? null : <polyline className="act-depth-chart__line" points={line} />}
    </svg>
  );
}
