// A still preview of the main light: a row of keycaps lit with the colour at
// the brightness. Custom shows a neutral pattern (the keyboard keeps its
// stored per-key colours). It's a picture of the draft only: nothing is sent
// to the keyboard.

import { useDict } from "../../i18n";
import type { LightModeView } from "../../ipc/bindings";
import { modeName } from "./labels";
import { previewOpacity } from "./model";

/** Keycaps in the strip. */
const KEYS = 12;
/** The neutral per-key pattern shown for Custom. */
const CUSTOM_PATTERN = ["#9aa3b2", "#6c7585", "#c3cad6", "#7f8899"];

/** Props of {@link LightPreview}. */
export interface LightPreviewProps {
  /** Shown above the strip ("On the keyboard now"). */
  caption: string;
  mode: LightModeView;
  /** "#rrggbb". */
  color: string;
  brightness: number;
  maxBrightness: number;
  /** Whether the effect shows the stored per-key colours (Custom). */
  perKey: boolean;
}

/** One preview strip with its caption. */
export function LightPreview({ caption, mode, color, brightness, maxBrightness, perKey }: LightPreviewProps) {
  const dict = useDict();
  const t = dict.lighting.preview;
  const opacity = previewOpacity(brightness, maxBrightness);
  const level = dict.lighting.level({ value: brightness, max: maxBrightness });
  const effect = modeName(dict, mode);
  const label = perKey
    ? t.describeCustom({ effect, brightness: level })
    : t.describe({ effect, color, brightness: level });
  const keys = Array.from({ length: KEYS }, (_, index) => index);
  return (
    <figure className="li-preview">
      <figcaption className="li-preview__caption ow-small ow-muted">{caption}</figcaption>
      <div className="li-preview__strip" role="img" aria-label={label}>
        {keys.map((index) => {
          const glow = perKey ? (CUSTOM_PATTERN[index % CUSTOM_PATTERN.length] ?? color) : color;
          return (
            <span key={index} className="li-preview__key">
              <span className="li-preview__glow" style={{ backgroundColor: glow, opacity }} />
              <span className="li-preview__cap" />
            </span>
          );
        })}
      </div>
    </figure>
  );
}
