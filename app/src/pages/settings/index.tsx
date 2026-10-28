// The Settings page and its tabs (`#/settings?tab=`): General, Background,
// Games, Profile names and About. One page header (its description follows
// the tab), then the tab bar and the tab.

import type { ComponentType } from "react";

import { PageHeader } from "../../components/Layout";
import { useDict } from "../../i18n";
import { useTab, type TabId } from "../../routes";
import { TabbedPage } from "../../shell/PageTabs";
import { AboutTab } from "./AboutTab";
import { BackgroundTab } from "./BackgroundTab";
import { GamesTab } from "./games/GamesTab";
import { GeneralTab } from "./GeneralTab";
import { ProfileNamesTab } from "./ProfileNamesTab";
import "./settings.css";

const TABS: Readonly<Record<TabId<"settings">, ComponentType>> = {
  general: GeneralTab,
  background: BackgroundTab,
  games: GamesTab,
  profiles: ProfileNamesTab,
  about: AboutTab,
};

/** The Settings page. */
export default function SettingsPage() {
  const dict = useDict();
  const [tab] = useTab("settings");
  const Tab = TABS[tab];
  return (
    <TabbedPage page="settings">
      <div className="st-page">
        <PageHeader title={dict.settings.title} description={dict.settings.descriptions[tab]} />
        <Tab />
      </div>
    </TabbedPage>
  );
}
