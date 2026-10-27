// The connection state in words (the header pill and the Keyboard page).

import type { Dict } from "../i18n";
import type { Format } from "../i18n/format";
import type { DeviceStatus } from "../ipc/bindings";

/** How the state looks. */
export type StatusTone = "ok" | "progress" | "warning" | "danger";

/** The one-line summary of the connection and its tone. */
export function connectionSummary(status: DeviceStatus, dict: Dict, format: Format): { text: string; tone: StatusTone } {
  const s = dict.shell.status;
  const c = status.connection;
  switch (c.state) {
    case "starting":
      return { text: s.starting, tone: "progress" };
    case "searching":
      return { text: s.searching, tone: "progress" };
    case "several_keyboards":
      return { text: s.severalKeyboards, tone: "warning" };
    case "opening":
      return { text: s.opening, tone: "progress" };
    case "open_failed":
      return { text: s.openFailed({ seconds: format.seconds(c.retry_in_ms) }), tone: "warning" };
    case "observing":
      return { text: s.observing, tone: "progress" };
    case "other_program":
      return { text: s.otherProgram, tone: "warning" };
    case "reading":
      return { text: s.reading({ done: c.done, total: c.total }), tone: "progress" };
    case "read_failed":
      return { text: s.readFailed, tone: "danger" };
    case "ready": {
      const model = status.device?.model ?? status.device?.product ?? dict.common.unknown;
      const firmware = status.device?.firmware ?? null;
      return {
        text: firmware === null ? s.readyNoFirmware({ model }) : s.ready({ model, firmware }),
        tone: "ok",
      };
    }
    case "lost":
      return { text: s.lost, tone: "danger" };
    case "reset_seen":
      return { text: s.resetSeen, tone: "danger" };
    case "unsupported":
      return { text: s.unsupported, tone: "warning" };
    case "released":
      return { text: s.released, tone: "warning" };
    case "fault":
      return { text: s.fault, tone: "danger" };
  }
}

/** Whether the user can ask to (re)connect in this state. */
export function canConnect(status: DeviceStatus): boolean {
  const state = status.connection.state;
  return (
    state === "released" ||
    state === "lost" ||
    state === "reset_seen" ||
    state === "read_failed" ||
    state === "open_failed" ||
    state === "searching"
  );
}
