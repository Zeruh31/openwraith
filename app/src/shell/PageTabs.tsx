// The tab bar of a page with tabs (`#/keys?tab=macros`). Tabs are links
// (each one is a route; the current one has `aria-current="page"`), drawn as
// tabs under the page's header; a dot marks a tab with unwritten changes.
// `TabbedPage` puts the bar under every `PageHeader` inside it.

import type { ReactNode } from "react";

import { PageHeaderSlot } from "../components/Layout";
import { useDict, type Dict } from "../i18n";
import { href, PAGE_TABS, useTab, type TabbedPageId } from "../routes";
import { useDraftTabs } from "../state/drafts";

/** The localized label of a page's tab ("Macros"); an unknown tab id is shown as it is. */
export function tabLabel(dict: Dict, page: TabbedPageId, tab: string): string {
  const pages: Readonly<Record<TabbedPageId, Readonly<Record<string, string>>>> = dict.shell.tabs.pages;
  return pages[page][tab] ?? tab;
}

/** The tab bar of `page`. */
export function PageTabs({ page }: { page: TabbedPageId }) {
  const dict = useDict();
  const [current] = useTab(page);
  const drafts = useDraftTabs(page);
  const tabs: readonly string[] = PAGE_TABS[page];
  const pageId: TabbedPageId = page;
  return (
    <nav className="ow-page-tabs" aria-label={dict.shell.tabs.label({ page: dict.shell.nav[pageId] })}>
      <ul className="ow-page-tabs__list">
        {tabs.map((tab) => {
          const selected = tab === current;
          const label = tabLabel(dict, pageId, tab);
          return (
            <li key={tab}>
              <a
                href={href({ page, params: { tab } })}
                className={`ow-page-tabs__tab${selected ? " ow-page-tabs__tab--current" : ""}`}
                aria-current={selected ? "page" : undefined}
                data-tab={tab}
              >
                <span>{label}</span>
                {drafts.includes(tab) ? (
                  <span className="ow-page-tabs__dot" title={dict.shell.tabs.unwritten}>
                    <span className="sr-only">{dict.shell.tabs.unwritten}</span>
                  </span>
                ) : null}
              </a>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

/** Props of {@link TabbedPage}. */
export interface TabbedPageProps {
  page: TabbedPageId;
  children?: ReactNode | undefined;
}

/** A page with tabs: every `PageHeader` inside it is followed by the page's tab bar. */
export function TabbedPage({ page, children }: TabbedPageProps) {
  return <PageHeaderSlot value={<PageTabs page={page} />}>{children}</PageHeaderSlot>;
}
