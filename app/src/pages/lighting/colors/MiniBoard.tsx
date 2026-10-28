// A small still picture of the W75 with each key in its colour (the review's
// before/after boards). It is one image with a text alternative that says
// which colours are on how many keys; the knob is drawn as a grey circle.
// Colours keep their values in Windows high contrast (forced-color-adjust).

import { KEYBOARD_HEIGHT_U, KEYBOARD_WIDTH_U, W75_GEOMETRY } from "../../../components/keyboard";
import { useDict } from "../../../i18n";
import { colorText } from "./labels";
import { colorCounts } from "./model";

/** Colours named in the text alternative before "and N more colours". */
const NAMED_COLORS = 4;

/** Props of {@link MiniBoard}. */
export interface MiniBoardProps {
  /** Shown above the board ("Now", "After writing"). */
  caption: string;
  /** Colours by key index. */
  colors: ReadonlyMap<number, string>;
  /** Keys outlined as changed. */
  changed?: ReadonlySet<number> | undefined;
  /** Pixels per key unit (default 18). */
  unitPx?: number | undefined;
}

/** A small board picture with its caption. */
export function MiniBoard({ caption, colors, changed, unitPx = 18 }: MiniBoardProps) {
  const dict = useDict();
  const t = dict.lighting.colors.review;
  const counts = colorCounts(colors);
  const parts = counts.slice(0, NAMED_COLORS).map((entry) => t.colorCount({ color: colorText(dict, entry.color), n: entry.count }));
  if (counts.length > NAMED_COLORS) parts.push(t.moreColors({ n: counts.length - NAMED_COLORS }));
  const label = t.picture({ caption, summary: parts.join("; ") });
  return (
    <figure className="li-mini">
      <figcaption className="li-mini__caption ow-small ow-muted">{caption}</figcaption>
      <div
        className="li-mini__board"
        role="img"
        aria-label={label}
        style={{ width: KEYBOARD_WIDTH_U * unitPx, height: KEYBOARD_HEIGHT_U * unitPx }}
      >
        {W75_GEOMETRY.map((shape) => {
          const color = colors.get(shape.index);
          const knob = shape.shape === "knob";
          return (
            <span
              key={shape.index}
              className={[
                "li-mini__key",
                knob ? "li-mini__key--knob" : "",
                changed?.has(shape.index) === true ? "li-mini__key--changed" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              data-key-index={shape.index}
              style={{
                left: shape.x * unitPx + 1,
                top: shape.y * unitPx + 1,
                width: shape.w * unitPx - 2,
                height: shape.h * unitPx - 2,
                backgroundColor: knob || color === undefined ? undefined : color,
              }}
            />
          );
        })}
      </div>
    </figure>
  );
}
