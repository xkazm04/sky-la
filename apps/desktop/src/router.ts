import { useEffect, useState } from "react";

/** Every screen. The order is the source list's order. */
export const SCREENS = [
  "overview",
  "inbox",
  "invoices",
  "bank",
  "statements",
  "taxes",
  "advisors",
  "register",
  "settings",
] as const;

export type Screen = (typeof SCREENS)[number];

/** A parsed location: `#/invoices/2026-114` → `{ screen: "invoices", item: "2026-114" }`. */
/** The steps before books are open. */
export const SESSION_SCREENS = ["setup", "unlock", "recover"] as const;
export type SessionScreen = (typeof SESSION_SCREENS)[number];

export interface Route {
  readonly screen: Screen | "gallery" | SessionScreen;
  readonly item: string | null;
}

export function parseRoute(hash: string): Route {
  const [first, ...rest] = hash.replace(/^#\/?/, "").split("/");
  const item = rest.length > 0 ? decodeURIComponent(rest.join("/")) : null;
  if (first === "gallery") return { screen: "gallery", item: null };
  const session = SESSION_SCREENS.find((s) => s === first);
  if (session) return { screen: session, item: null };
  const screen = SCREENS.find((s) => s === first) ?? "overview";
  return { screen, item: screen === first ? item : null };
}

export function href(screen: Screen | "gallery" | SessionScreen, item?: string | null): string {
  return item ? `#/${screen}/${encodeURIComponent(item)}` : `#/${screen}`;
}

/** Moves to a screen (and optionally selects an item) without a page load. */
export function navigate(
  screen: Screen | "gallery" | SessionScreen,
  item?: string | null,
  replace = false,
): void {
  const next = href(screen, item);
  if (globalThis.location.hash === next) return;
  if (replace) globalThis.history.replaceState(null, "", next);
  else globalThis.history.pushState(null, "", next);
  globalThis.dispatchEvent(new HashChangeEvent("hashchange"));
}

/** The current route; re-renders on navigation and Back/Forward. */
export function useRoute(): Route {
  const [route, setRoute] = useState(() => parseRoute(globalThis.location.hash));
  useEffect(() => {
    const update = () => setRoute(parseRoute(globalThis.location.hash));
    globalThis.addEventListener("hashchange", update);
    globalThis.addEventListener("popstate", update);
    return () => {
      globalThis.removeEventListener("hashchange", update);
      globalThis.removeEventListener("popstate", update);
    };
  }, []);
  return route;
}
