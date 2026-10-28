// The Lighting page's tabs (`#/lighting?tab=`): Effect (the existing page)
// and Key colours. Each tab's own header is followed by the tab bar
// (TabbedPage). Owned by the lighting page agent from here on.

import { lazy, Suspense } from "react";

import { Spinner } from "../../components/Feedback";
import { useDict } from "../../i18n";
import { useTab } from "../../routes";
import { TabbedPage } from "../../shell/PageTabs";

const TABS = {
  effect: lazy(() => import("./index")),
  colors: lazy(() => import("./colors")),
};

/** The Lighting page with its tabs. */
export default function LightingTabs() {
  const dict = useDict();
  const [tab] = useTab("lighting");
  const Tab = TABS[tab];
  return (
    <TabbedPage page="lighting">
      <Suspense fallback={<Spinner label={dict.common.loading} size={24} />}>
        <Tab />
      </Suspense>
    </TabbedPage>
  );
}
