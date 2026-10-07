import { useEffect, useState } from "react";

/** The current hash route without the leading `#/`, e.g. `gallery`. */
export function currentRoute(): string {
  return globalThis.location?.hash.replace(/^#\/?/, "") ?? "";
}

/** Re-renders on hash changes. WP-10 replaces this with the app's router. */
export function useRoute(): string {
  const [route, setRoute] = useState(currentRoute);
  useEffect(() => {
    const update = () => setRoute(currentRoute());
    globalThis.addEventListener("hashchange", update);
    return () => globalThis.removeEventListener("hashchange", update);
  }, []);
  return route;
}
