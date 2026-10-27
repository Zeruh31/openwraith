// The app frame: header, navigation, banners, the routed page (lazy, each in
// its own error boundary), toasts, the close guard and the simulator panel.

import { lazy, Suspense, useEffect, useRef, type ComponentType, type LazyExoticComponent, type ReactNode } from "react";

import { ErrorBoundary } from "../components/ErrorBoundary";
import { Spinner } from "../components/Feedback";
import { ToastRegion } from "../components/Toasts";
import { useDict } from "../i18n";
import { rememberRoute, useRoute, type PageId } from "../routes";
import { useAppInfo } from "../state/appInfo";
import { useStatus } from "../state/status";
import { BannerStack } from "./BannerStack";
import { CloseGuard } from "./CloseGuard";
import { DevSimPanel } from "./DevSimPanel";
import { Header } from "./Header";
import { LastReportDialog } from "./LastReportDialog";
import { Nav } from "./Nav";
import { NoticeToaster } from "./NoticeToaster";

/** The pages, loaded on first visit; a broken page breaks only its own route. */
const PAGES: Record<PageId, LazyExoticComponent<ComponentType>> = {
  device: lazy(() => import("../pages/device")),
  actuation: lazy(() => import("../pages/actuation")),
  lighting: lazy(() => import("../pages/lighting")),
  backup: lazy(() => import("../pages/backup")),
  settings: lazy(() => import("../pages/settings")),
};

/** Props of {@link AppShell}. */
export interface AppShellProps {
  /** Extra tools rendered on top (the browser preview's scenario panel). */
  devtools?: ReactNode | undefined;
}

/** The app frame around the current page. */
export function AppShell({ devtools }: AppShellProps) {
  const dict = useDict();
  const route = useRoute();
  const info = useAppInfo();
  const status = useStatus();
  const mainRef = useRef<HTMLElement>(null);
  const Page = PAGES[route.page];

  useEffect(() => {
    rememberRoute(route);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only the page matters
  }, [route.page]);

  return (
    <div className={info.sim || status.sim ? "ow-app ow-app--sim" : "ow-app"}>
      <button
        type="button"
        className="ow-skip"
        onClick={() => {
          mainRef.current?.focus();
        }}
      >
        {dict.shell.skipToContent}
      </button>
      <Header />
      <div className="ow-app__body">
        <Nav />
        <main ref={mainRef} className="ow-main" tabIndex={-1}>
          <BannerStack />
          <div className="ow-page">
            <ErrorBoundary resetKey={route.page}>
              <Suspense
                fallback={
                  <div className="ow-page-loading">
                    <Spinner label={dict.common.loading} size={24} />
                  </div>
                }
              >
                <Page />
              </Suspense>
            </ErrorBoundary>
          </div>
        </main>
      </div>
      <ToastRegion />
      <CloseGuard />
      <LastReportDialog />
      <NoticeToaster />
      {/* Simulator builds run on the Vite dev server; a production bundle drops the panel. */}
      {import.meta.env.DEV && (info.sim || status.sim) ? <DevSimPanel /> : null}
      {devtools}
    </div>
  );
}
