// The shape of the browser-preview fixtures (`fixtures/*.ts`), which the
// core's tests generate by driving the real worker on the simulator.

import type {
  AppError,
  AppEvent,
  AppInfo,
  DeviceSnapshot,
  DeviceStatus,
  ProgressEvent,
  SafetyBackupEntry,
  Settings,
} from "../ipc/bindings";
import type { Args, CommandName, Results } from "../ipc/commands";

/** One recorded command call: its arguments, result or error, and progress. */
export type Recorded = {
  [K in CommandName]: {
    command: K;
    args: Args[K];
    ok: Results[K] | null;
    err: AppError | null;
    progress: ProgressEvent[];
    /** Virtual time of the call since the fixture's start. */
    at_ms: number;
  };
}[CommandName];

/** A recorded scenario. */
export interface Fixture {
  id: string;
  app_info: AppInfo;
  settings: Settings;
  initial_status: DeviceStatus;
  initial_snapshot: DeviceSnapshot | null;
  /** Events pushed on the `subscribe` channel, by virtual time. */
  events: { at_ms: number; event: AppEvent }[];
  calls: Recorded[];
  safety_backups: SafetyBackupEntry[];
  ui?: Record<string, unknown>;
}

/**
 * A preview scenario composed by a page (`scenarios/<page>.ts`): a fixture,
 * the route to open and optional UI state for the page.
 */
export interface PreviewScenario {
  /** Unique id, used in `?scenario=<id>`. */
  id: string;
  /** The fixture id (`fixtures/<id>.ts`). */
  fixture: string;
  /** The route to open, e.g. `#/actuation`. */
  route: string;
  /** Page UI state by page key, read with `previewUi(pageKey)` (e.g. preselected keys). */
  ui?: Record<string, unknown> | undefined;
  /** Start from the fixture's final recorded state instead of its initial one. */
  startAtEnd?: boolean | undefined;
}
