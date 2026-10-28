// The Actuation page's tabs (`#/actuation?tab=`): Actuation & Rapid
// Trigger (the existing page), Dead band, SOCD and Key depth. Each tab's
// own header is followed by the tab bar (TabbedPage). Owned by the
// actuation page agent from here on.

import { lazy, Suspense } from "react";

import { Spinner } from "../../components/Feedback";
import { useDict } from "../../i18n";
import { useTab } from "../../routes";
import { TabbedPage } from "../../shell/PageTabs";

const TABS = {
  trigger: lazy(() => import("./index")),
  deadband: lazy(() => import("./deadband")),
  socd: lazy(() => import("./socd")),
  depth: lazy(() => import("./depth")),
};

/** The Actuation page with its tabs. */
export default function ActuationTabs() {
  const dict = useDict();
  const [tab] = useTab("actuation");
  const Tab = TABS[tab];
  return (
    <TabbedPage page="actuation">
      <Suspense fallback={<Spinner label={dict.common.loading} size={24} />}>
        <Tab />
      </Suspense>
    </TabbedPage>
  );
}
