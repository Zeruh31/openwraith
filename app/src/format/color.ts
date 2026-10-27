// Colours: the IPC uses lowercase "#rrggbb".

/** An sRGB colour with 0-255 channels. */
export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** Normalises "#abc", "abc", "#AABBCC" or "aabbcc" to "#aabbcc"; `null` when it is not a colour. */
export function normalizeHex(text: string): string | null {
  const raw = text.trim().replace(/^#/, "").toLowerCase();
  if (/^[0-9a-f]{3}$/.test(raw)) {
    return `#${raw
      .split("")
      .map((c) => c + c)
      .join("")}`;
  }
  if (/^[0-9a-f]{6}$/.test(raw)) {
    return `#${raw}`;
  }
  return null;
}

/** Whether `text` is exactly a lowercase "#rrggbb" colour. */
export function isHexColor(text: string): boolean {
  return /^#[0-9a-f]{6}$/.test(text);
}

/** The channels of a colour, or `null`. */
export function hexToRgb(text: string): Rgb | null {
  const hex = normalizeHex(text);
  if (hex === null) {
    return null;
  }
  return {
    r: parseInt(hex.slice(1, 3), 16),
    g: parseInt(hex.slice(3, 5), 16),
    b: parseInt(hex.slice(5, 7), 16),
  };
}

function channel(value: number): number {
  const c = value / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** WCAG relative luminance (0..1); 0 for something that is not a colour. */
export function relativeLuminance(text: string): number {
  const rgb = hexToRgb(text);
  if (rgb === null) {
    return 0;
  }
  return 0.2126 * channel(rgb.r) + 0.7152 * channel(rgb.g) + 0.0722 * channel(rgb.b);
}

/** WCAG contrast ratio between two colours (1..21). */
export function contrastRatio(a: string, b: string): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  const [hi, lo] = la > lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}
