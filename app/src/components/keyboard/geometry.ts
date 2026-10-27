// OpenWraith's own W75 geometry, in key units (1 u = one standard key),
// derived from the key positions in docs/PROTOCOL.md section 5 and standard
// 75 % layout conventions, snapped to a 0.25 u grid. No vendor layout file or
// image is used. The indices are the library's key indices
// (`generated/w75Keys.ts`).

/** One key on the drawing. */
export interface KeyShape {
  /** The library's key index (row × 22 + col). */
  index: number;
  /** Left edge, in key units. */
  x: number;
  /** Top edge, in key units. */
  y: number;
  /** Width, in key units. */
  w: number;
  /** Height, in key units. */
  h: number;
  /** The rotary knob is drawn as a circle and is never selectable. */
  shape: "key" | "knob";
}

/** Width of the drawing, in key units. */
export const KEYBOARD_WIDTH_U = 16.25;
/** Height of the drawing, in key units. */
export const KEYBOARD_HEIGHT_U = 6.5;
/** The knob's key index. */
export const KNOB_INDEX = 15;

const k = (index: number, x: number, y: number, w = 1): KeyShape => ({ index, x, y, w, h: 1, shape: "key" });

/** Every key of the W75 (82, the knob included), row by row. */
export const W75_GEOMETRY: readonly KeyShape[] = [
  // Row 0: Esc, F1-F12 in groups of four, Del, knob.
  k(0, 0, 0),
  k(2, 1.25, 0),
  k(3, 2.25, 0),
  k(4, 3.25, 0),
  k(5, 4.25, 0),
  k(6, 5.5, 0),
  k(7, 6.5, 0),
  k(8, 7.5, 0),
  k(9, 8.5, 0),
  k(10, 9.75, 0),
  k(11, 10.75, 0),
  k(12, 11.75, 0),
  k(13, 12.75, 0),
  k(14, 14, 0),
  { index: KNOB_INDEX, x: 15.3, y: 0.05, w: 0.9, h: 0.9, shape: "knob" },
  // Row 1: ` 1-0 - = Backspace Home.
  k(22, 0, 1.25),
  k(23, 1, 1.25),
  k(24, 2, 1.25),
  k(25, 3, 1.25),
  k(26, 4, 1.25),
  k(27, 5, 1.25),
  k(28, 6, 1.25),
  k(29, 7, 1.25),
  k(30, 8, 1.25),
  k(31, 9, 1.25),
  k(32, 10, 1.25),
  k(33, 11, 1.25),
  k(34, 12, 1.25),
  k(36, 13, 1.25, 2),
  k(37, 15.25, 1.25),
  // Row 2: Tab Q-P [ ] \ PgUp.
  k(44, 0, 2.25, 1.5),
  k(45, 1.5, 2.25),
  k(46, 2.5, 2.25),
  k(47, 3.5, 2.25),
  k(48, 4.5, 2.25),
  k(49, 5.5, 2.25),
  k(50, 6.5, 2.25),
  k(51, 7.5, 2.25),
  k(52, 8.5, 2.25),
  k(53, 9.5, 2.25),
  k(54, 10.5, 2.25),
  k(55, 11.5, 2.25),
  k(56, 12.5, 2.25),
  k(58, 13.5, 2.25, 1.5),
  k(59, 15.25, 2.25),
  // Row 3: Caps A-' Enter PgDn.
  k(66, 0, 3.25, 1.75),
  k(68, 1.75, 3.25),
  k(69, 2.75, 3.25),
  k(70, 3.75, 3.25),
  k(71, 4.75, 3.25),
  k(72, 5.75, 3.25),
  k(73, 6.75, 3.25),
  k(74, 7.75, 3.25),
  k(75, 8.75, 3.25),
  k(76, 9.75, 3.25),
  k(77, 10.75, 3.25),
  k(78, 11.75, 3.25),
  k(80, 12.75, 3.25, 2.25),
  k(81, 15.25, 3.25),
  // Row 4: L-Shift Z-/ R-Shift Up (the arrow cluster sits a quarter row lower).
  k(88, 0, 4.25, 2.25),
  k(90, 2.25, 4.25),
  k(91, 3.25, 4.25),
  k(92, 4.25, 4.25),
  k(93, 5.25, 4.25),
  k(94, 6.25, 4.25),
  k(95, 7.25, 4.25),
  k(96, 8.25, 4.25),
  k(97, 9.25, 4.25),
  k(98, 10.25, 4.25),
  k(99, 11.25, 4.25),
  k(101, 12.25, 4.25, 1.75),
  k(102, 14.25, 4.5),
  // Row 5: modifiers, space, R-Alt Fn R-Ctrl, arrows.
  k(110, 0, 5.25, 1.25),
  k(111, 1.25, 5.25, 1.25),
  k(112, 2.5, 5.25, 1.25),
  k(116, 3.75, 5.25, 6.25),
  k(120, 10, 5.25),
  k(121, 11, 5.25),
  k(122, 12, 5.25),
  k(123, 13.25, 5.5),
  k(124, 14.25, 5.5),
  k(125, 15.25, 5.5),
];

const BY_INDEX = new Map(W75_GEOMETRY.map((shape) => [shape.index, shape]));

/** The shape of a key, if it is on the W75. */
export function shapeOf(index: number): KeyShape | undefined {
  return BY_INDEX.get(index);
}

/** The centre of a key, in key units. */
export function centerOf(shape: KeyShape): { x: number; y: number } {
  return { x: shape.x + shape.w / 2, y: shape.y + shape.h / 2 };
}
