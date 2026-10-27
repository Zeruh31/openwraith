// Command errors: every rejection of a command is an `AppError` (see
// `crates/openwraith-app-core/src/dto/error.rs`); anything else is turned
// into one of kind `internal`.

import type { AppError } from "./bindings";

/** Whether `value` has the shape of an `AppError`. */
export function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const record = value as { kind?: unknown; message?: unknown; detail?: unknown };
  return (
    typeof record.kind === "string" &&
    typeof record.message === "string" &&
    (record.detail === null || typeof record.detail === "object")
  );
}

/** `value` as an `AppError` (kind `internal` when it is something else). */
export function toAppError(value: unknown): AppError {
  if (isAppError(value)) {
    return value;
  }
  if (value instanceof Error) {
    return { kind: "internal", detail: null, message: value.message };
  }
  if (typeof value === "string") {
    return { kind: "internal", detail: null, message: value };
  }
  // `JSON.stringify` returns undefined for undefined and functions.
  const json = JSON.stringify(value) as string | undefined;
  return { kind: "internal", detail: null, message: json ?? "unknown error" };
}
