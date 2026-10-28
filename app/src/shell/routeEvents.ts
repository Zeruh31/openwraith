// Routes sent from outside the page: a click on a Windows notification or a
// tray item shows the window and then asks the page to open a route
// (`#/backup?recover=1`, `#/settings?tab=games`, ...). The Rust side
// dispatches a DOM event on the window (no IPC permission needed):
//
//   window.dispatchEvent(new CustomEvent("openwraith:navigate", { detail: "#/backup?recover=1" }))
//
// Only the app's own routes are opened: an unknown page gives the Keyboard
// page, like any other hash, and anything that isn't a route is ignored.

import { navigate, parseRoute, type RouteTarget } from "../routes";

/** The name of the DOM event (`CustomEvent<string>`, `detail` = the route). */
export const ROUTE_EVENT = "openwraith:navigate";

/** The route a route event asks for, or `null` when it carries none (`"#/x?y=1"` or `"/x?y=1"`). */
export function routeFromEvent(detail: unknown): RouteTarget | null {
  if (typeof detail !== "string") return null;
  const text = detail.trim();
  if (!text.startsWith("#/") && !text.startsWith("/")) return null;
  const route = parseRoute(text.startsWith("#") ? text : `#${text}`);
  return { page: route.page, params: { ...route.params } };
}

/** Moves the keyboard focus to the page's main region, so the new route is where the user lands. */
function focusMain(): void {
  document.querySelector<HTMLElement>("main")?.focus();
}

/** Opens the routes that route events ask for; returns the function that stops listening. */
export function startRouteEvents(): () => void {
  const onRoute = (event: Event) => {
    const target = routeFromEvent(event instanceof CustomEvent ? (event as CustomEvent<unknown>).detail : null);
    if (target === null) return;
    navigate(target);
    focusMain();
  };
  window.addEventListener(ROUTE_EVENT, onRoute);
  return () => {
    window.removeEventListener(ROUTE_EVENT, onRoute);
  };
}
