// Facts about this build (version, simulator, safety folder), read once at start.

import type { AppInfo } from "../ipc/bindings";
import { createStore, useStore } from "./store";

/** Shown until `app_info` has answered. */
export const DEFAULT_APP_INFO: AppInfo = {
  version: "",
  sim: false,
  platform: "windows",
  os_locale: null,
  safety_dir: { state: "ok", path: "" },
  tested_firmware: [],
  repository: "",
};

/** The app's facts. */
export const appInfoStore = createStore<AppInfo>(DEFAULT_APP_INFO);

/** The app's facts; re-renders when they arrive. */
export function useAppInfo(): AppInfo {
  return useStore(appInfoStore);
}

/** The safety-backup folder, or `null` when it is unavailable. */
export function safetyDirPath(info: AppInfo): string | null {
  return info.safety_dir.state === "ok" ? info.safety_dir.path : null;
}
