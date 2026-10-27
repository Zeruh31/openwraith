// The page navigation (an icon rail on narrow windows). A dot marks pages
// with unwritten changes.

import { Icon, type IconName } from "../components/Icon";
import { useDict } from "../i18n";
import { href, useRoute, type PageId } from "../routes";
import { useDraftPages } from "../state/drafts";

const ITEMS: readonly { page: PageId; icon: IconName }[] = [
  { page: "device", icon: "keyboard" },
  { page: "actuation", icon: "device" },
  { page: "lighting", icon: "light" },
  { page: "backup", icon: "backup" },
];

/** The navigation rail. */
export function Nav() {
  const dict = useDict();
  const route = useRoute();
  const drafts = useDraftPages();
  const link = (page: PageId, icon: IconName) => {
    const current = route.page === page;
    const label = dict.shell.nav[page];
    const draft = drafts.includes(page);
    return (
      <li key={page}>
        <a
          href={href(page)}
          className={`ow-nav__link${current ? " ow-nav__link--current" : ""}`}
          aria-current={current ? "page" : undefined}
          title={label}
        >
          <Icon name={icon} size={20} />
          <span className="ow-nav__label">{label}</span>
          {draft ? (
            <span className="ow-nav__dot" title={dict.shell.nav.unwritten}>
              <span className="sr-only">{dict.shell.nav.unwritten}</span>
            </span>
          ) : null}
        </a>
      </li>
    );
  };
  return (
    <nav className="ow-nav" aria-label={dict.shell.nav.label}>
      <ul className="ow-nav__list">{ITEMS.map((item) => link(item.page, item.icon))}</ul>
      <ul className="ow-nav__list ow-nav__list--bottom">{link("settings", "settings")}</ul>
    </nav>
  );
}
