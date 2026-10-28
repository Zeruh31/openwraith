// The root component: start-up (app facts, settings, event subscription),
// then the app shell. Route events (a click on a Windows notification or a
// tray item) are followed from the start, so one sent while the app starts
// opens its page once the shell is shown.

import { useEffect, type ReactNode } from "react";

import { Button } from "./components/Button";
import { Spinner } from "./components/Feedback";
import { ErrorView } from "./flow/ErrorView";
import { useDict } from "./i18n";
import { restoreLastRoute } from "./routes";
import { AppLogo } from "./shell/Header";
import { AppShell } from "./shell/AppShell";
import { startRouteEvents } from "./shell/routeEvents";
import { boot, retryBoot, useBoot } from "./state/boot";

/** Props of {@link App}. */
export interface AppProps {
  /** Extra tools (the browser preview's scenario panel). */
  devtools?: ReactNode | undefined;
}

/** The app. */
export function App({ devtools }: AppProps) {
  const dict = useDict();
  const state = useBoot();

  useEffect(() => {
    restoreLastRoute();
    void boot();
  }, []);

  useEffect(() => startRouteEvents(), []);

  if (state.phase === "ready") {
    return <AppShell devtools={devtools} />;
  }
  return (
    <div className="ow-boot">
      <AppLogo size={56} />
      {state.phase === "loading" ? (
        <p className="ow-row">
          <Spinner />
          <span>{dict.shell.boot.loading}</span>
        </p>
      ) : (
        <div className="ow-boot__error">
          <h1>{dict.shell.boot.failedTitle}</h1>
          <p>{dict.shell.boot.failedBody}</p>
          <ErrorView error={state.error} written="none" />
          <Button
            variant="primary"
            icon="refresh"
            onClick={() => {
              void retryBoot();
            }}
          >
            {dict.common.actions.retry}
          </Button>
        </div>
      )}
      {devtools}
    </div>
  );
}
