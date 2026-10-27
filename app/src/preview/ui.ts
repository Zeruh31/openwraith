// Page UI state for browser-preview scenarios (e.g. preselected keys).
// Outside the preview (`vite --mode mock`) it is always `undefined`.

let current: Readonly<Record<string, unknown>> | undefined;

/** Sets the scenario's page UI state (called by the preview runtime only). */
export function setPreviewUi(ui: Readonly<Record<string, unknown>> | undefined): void {
  current = ui;
}

/**
 * The preview scenario's UI state for `pageKey`, or `undefined` outside the
 * preview. The page knows the shape it put into its own scenarios.
 */
// eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters -- the page names the shape it stored
export function previewUi<T>(pageKey: string): T | undefined {
  if (import.meta.env.MODE !== "mock") {
    return undefined;
  }
  return current?.[pageKey] as T | undefined;
}
