// Dictionary typing: the English files are the shape source (`as const`
// objects of strings and message functions); every Turkish file is typed
// `Messages<typeof english>`, so a missing or extra key is a compile error.

/** A UI language. */
export type Lang = "en" | "tr";

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
