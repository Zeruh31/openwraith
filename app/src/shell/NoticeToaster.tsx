// Turns the backend's one-shot notices into toasts (plan invalidations are
// handled by the open write flow instead).

import { useRef } from "react";

import { useDict } from "../i18n";
import { useNotices } from "../state/status";
import { toast } from "../state/toasts";

/** Shows notices as toasts; renders nothing. */
export function NoticeToaster() {
  const dict = useDict();
  const lostOnce = useRef(false);
  useNotices((notice) => {
    switch (notice.notice) {
      case "profile_changed_on_keyboard":
        toast({
          kind: "info",
          title: dict.shell.notices.profileChanged({ to: notice.to }),
          body: dict.shell.notices.profileChangedBody,
        });
        break;
      case "disconnected":
        lostOnce.current = true;
        toast({ kind: "warning", title: dict.shell.notices.disconnected });
        break;
      case "connected":
        // Only after a disconnection: the first connection needs no toast.
        if (lostOnce.current) toast({ kind: "success", title: dict.shell.notices.connected });
        break;
      case "socd_toggled_on_keyboard":
        // The SOCD power is one setting of the whole keyboard (Fn + Right Shift).
        toast({ kind: "info", title: dict.shell.notices.socdToggled({ on: notice.power }) });
        break;
      case "plan_invalidated":
      case "depth_view_stopped":
      case "stray_travel_test":
        // Handled by the open write flow, the key-depth tab and the banner.
        break;
    }
  });
  return null;
}
