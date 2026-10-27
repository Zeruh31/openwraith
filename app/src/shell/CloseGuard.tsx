// Shown when the window is closed while a write runs: the backend deferred
// the close and asks "keep open" or "stop safely and close".

import { useRef, useState } from "react";

import { Button } from "../components/Button";
import { Dialog } from "../components/Dialog";
import { useDict } from "../i18n";
import type { CloseChoice } from "../ipc/bindings";
import { api } from "../ipc/commands";
import { useStatus } from "../state/status";
import { toastError } from "./actions";

/** The close guard dialog. */
export function CloseGuard() {
  const dict = useDict();
  const status = useStatus();
  const keepRef = useRef<HTMLButtonElement>(null);
  const [stopping, setStopping] = useState(false);
  const open = status.close_requested;

  const answer = (choice: CloseChoice) => {
    if (choice === "stop_and_close") setStopping(true);
    api.answerClose(choice).catch((error: unknown) => {
      setStopping(false);
      toastError(error);
    });
  };

  // The × keeps the app open, and its name says so: this dialog exists
  // because the user tried to close the window.
  return (
    <Dialog
      open={open}
      title={dict.shell.closeGuard.title}
      onClose={() => {
        answer("keep_open");
      }}
      dismissable={!stopping}
      closeLabel={dict.shell.closeGuard.keepOpen}
      size="sm"
      initialFocus={keepRef}
      actions={
        <>
          <Button
            ref={keepRef}
            variant="primary"
            disabled={stopping}
            onClick={() => {
              answer("keep_open");
            }}
          >
            {dict.shell.closeGuard.keepOpen}
          </Button>
          <Button
            disabled={stopping}
            onClick={() => {
              answer("stop_and_close");
            }}
          >
            {dict.shell.closeGuard.stopAndClose}
          </Button>
        </>
      }
    >
      <p>{stopping ? dict.shell.closeGuard.stopping : dict.shell.closeGuard.body}</p>
    </Dialog>
  );
}
