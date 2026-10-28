// Turns the backend's one-shot notices into toasts (plan invalidations are
// handled by the open write flow instead; the lasting states, such as a
// suspected reset or an OpenWraith command using the keyboard, also have a
// banner). Notices that arrived before the shell was shown (the first
// `subscribe` comes first) are shown when it mounts.

import { useEffect, useRef } from "react";

import { getDict, type Dict } from "../i18n";
import type { FirmwareView, Notice } from "../ipc/bindings";
import { backgroundStore } from "../state/background";
import { currentProduct, namesOf, profileNamesStore } from "../state/profileNames";
import { statusStore, takeUndeliveredNotices, useNotices } from "../state/status";
import { toast } from "../state/toasts";

/** "3.22.07", or "unknown". */
function firmwareText(firmware: FirmwareView, dict: Dict): string {
  return firmware.version ?? dict.common.unknown;
}

/** The game's name in a notice (a rule without a label is "A game"). */
function gameLabel(label: string | null, dict: Dict): string {
  return label ?? dict.settings.games.recent.aGame;
}

/** Whether OpenWraith should mention automatic switches (the "game-switch notifications" setting, off by default). */
function autoSwitchToasts(): boolean {
  return backgroundStore.get()?.notifications.auto_switch === true;
}

/** The local name of a profile of the connected keyboard, for toasts. */
function nameOf(profile: number): string | null {
  const view = profileNamesStore.get();
  return namesOf(view, currentProduct(view, statusStore.get().device?.product ?? null))[profile - 1] ?? null;
}

/** Shows one notice as a toast (or nothing); `lost` remembers a disconnection for the next "connected". */
export function toastNotice(notice: Notice, lost: { current: boolean }): void {
  const dict = getDict();
  const n = dict.shell.notices;
  switch (notice.notice) {
    case "profile_changed_on_keyboard":
      toast({
        kind: "info",
        title: n.profileChanged({ to: notice.to }),
        body: n.profileChangedBody,
      });
      break;
    case "disconnected":
      lost.current = true;
      toast({ kind: "warning", title: n.disconnected });
      break;
    case "connected":
      // Only after a disconnection: the first connection needs no toast.
      if (lost.current) toast({ kind: "success", title: n.connected });
      break;
    case "socd_toggled_on_keyboard":
      // The SOCD power is one setting of the whole keyboard (Fn + Right Shift).
      toast({ kind: "info", title: n.socdToggled({ on: notice.power }) });
      break;
    case "reset_suspected":
      toast({ kind: "danger", title: n.resetSuspected({ n: notice.profile }), body: n.resetSuspectedBody });
      break;
    case "firmware_changed":
      toast({
        kind: "warning",
        title: n.firmwareChanged({ known: firmwareText(notice.known, dict), current: firmwareText(notice.current, dict) }),
        body: n.firmwareChangedBody,
      });
      break;
    case "settings_file_invalid":
      // Shown until dismissed: the user's settings were replaced by the defaults.
      toast({
        kind: "warning",
        title: n.settingsInvalid,
        body: notice.kept_as === null ? n.settingsInvalidLost : n.settingsInvalidKept({ file: notice.kept_as }),
        timeoutMs: 0,
      });
      break;
    case "auto_backup_failed":
      toast({
        kind: "warning",
        title: n.autoBackupFailed({ n: notice.profile }),
        body: n.autoBackupFailedBody,
        details: notice.message,
      });
      break;
    case "auto_switched": {
      if (!autoSwitchToasts()) break;
      const name = nameOf(notice.to);
      const title =
        notice.cause === "game_exited"
          ? n.autoSwitchedBack({ n: notice.to })
          : n.autoSwitched({ label: gameLabel(notice.label, dict), n: notice.to });
      toast({ kind: "info", title, body: name === null ? undefined : dict.common.profileNamed({ n: notice.to, name }) });
      break;
    }
    case "auto_switch_deferred": {
      const reason = dict.shell.automation.waitReasons[notice.reason];
      toast({
        kind: "info",
        title:
          notice.cause === "game_exited"
            ? n.autoWaitingBack({ n: notice.profile })
            : n.autoWaiting({ label: gameLabel(notice.label, dict), n: notice.profile }),
        body: n.autoWaitingBody({ reason }),
      });
      break;
    }
    case "auto_switch_skipped": {
      const reason = dict.shell.automation.waitReasons[notice.reason];
      toast({
        kind: "warning",
        title:
          notice.cause === "game_exited"
            ? n.autoSkippedBack({ n: notice.profile })
            : n.autoSkipped({ label: gameLabel(notice.label, dict), n: notice.profile }),
        body: n.autoSkippedBody({ reason }),
      });
      break;
    }
    case "auto_profile_paused":
      // A pause the user chose needs no toast; the others have a banner too.
      if (notice.reason.reason === "user") break;
      toast({
        kind: "warning",
        title: n.autoPaused,
        body: notice.reason.reason === "rate_limit" ? n.autoPausedRateLimit : n.autoPausedFailed,
        details: notice.reason.reason === "switch_failed" ? notice.reason.message : undefined,
      });
      break;
    case "paused_for_cli":
      // The banner says it while it lasts.
      break;
    case "plan_invalidated":
    case "depth_view_stopped":
    case "stray_travel_test":
      // Handled by the open write flow, the key-depth tab and the banner.
      break;
  }
}

/** Shows notices as toasts; renders nothing. */
export function NoticeToaster() {
  const lostOnce = useRef(false);
  useNotices((notice) => {
    toastNotice(notice, lostOnce);
  });
  useEffect(() => {
    for (const notice of takeUndeliveredNotices()) toastNotice(notice, lostOnce);
  }, []);
  return null;
}
