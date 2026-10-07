/** The user's appearance choice. `system` follows the OS setting. */
export type Appearance = "system" | "light" | "dark";

/** What is actually shown. */
export type ResolvedAppearance = "light" | "dark";

const STORAGE_KEY = "sky-la.appearance";
const DARK_QUERY = "(prefers-color-scheme: dark)";

export function resolveAppearance(choice: Appearance, systemIsDark: boolean): ResolvedAppearance {
  if (choice === "system") return systemIsDark ? "dark" : "light";
  return choice;
}

/** The stored choice, or `system`. Storage can be unavailable; that's fine. */
export function storedAppearance(): Appearance {
  try {
    const value = globalThis.localStorage?.getItem(STORAGE_KEY);
    return value === "light" || value === "dark" ? value : "system";
  } catch {
    return "system";
  }
}

export function storeAppearance(choice: Appearance): void {
  try {
    if (choice === "system") globalThis.localStorage?.removeItem(STORAGE_KEY);
    else globalThis.localStorage?.setItem(STORAGE_KEY, choice);
  } catch {
    // A per-viewer convenience only.
  }
}

/**
 * Sets `data-appearance` on the root element and keeps it in step with the
 * OS while the choice is `system`. Returns a function that stops listening.
 */
export function applyAppearance(choice: Appearance, root: HTMLElement = document.documentElement) {
  const query = globalThis.matchMedia?.(DARK_QUERY);
  const apply = () => {
    root.dataset.appearance = resolveAppearance(choice, query?.matches ?? false);
  };
  apply();
  if (choice !== "system" || !query) return () => {};
  query.addEventListener("change", apply);
  return () => query.removeEventListener("change", apply);
}
