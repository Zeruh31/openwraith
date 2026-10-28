// Dictionary typing: the English files are the shape source (`as const`
// objects of strings and message functions); every Turkish file is typed
// `Messages<typeof english>`, so a missing or extra key is a compile error.

/** A UI language. */
export type Lang = "en" | "tr";

/**
 * The dictionary areas (one file per area and language: `en/<area>.ts`,
 * `tr/<area>.ts`). Each page tab with its own texts has its own area, so
 * that page agents edit only their own files:
 * - `keys`: words shared by the Keys tabs and the shared action components;
 * - `keysAssign`, `keysMacros`, `keysAdvanced`: the Keys tabs' own texts;
 * - `actuation` (trigger, dead band, SOCD, key depth, presets), `lighting`
 *   (effect, key colours), `backupRestore` (backup, restore, share a profile):
 *   one area per page.
 */
export type Namespace =
  | "common"
  | "shell"
  | "device"
  | "settings"
  | "flow"
  | "errors"
  | "backupRestore"
  | "actuation"
  | "lighting"
  | "keys"
  | "keysAssign"
  | "keysMacros"
  | "keysAdvanced";

/**
 * The shape of a dictionary: string literals widen to `string`, message
 * functions keep their parameters, nested objects recurse.
 */
export type Messages<T> = {
  readonly [K in keyof T]: T[K] extends string
    ? string
    : T[K] extends (...args: infer A) => string
      ? (...args: A) => string
      : Messages<T[K]>;
};
