import { IpcError } from "@skyla/ipc";
import { useEffect, useState } from "react";

export type Query<T> =
  | { readonly state: "loading" }
  | { readonly state: "error"; readonly message: string }
  | { readonly state: "ready"; readonly data: T };

const cache = new Map<string, Promise<unknown>>();

/**
 * Runs a core command once per key and shares the answer. The core is the
 * source of truth; the webview only caches what it said for this session.
 */
export function useQuery<T>(key: string, run: () => Promise<T>): Query<T> {
  const [query, setQuery] = useState<Query<T>>({ state: "loading" });
  // biome-ignore lint/correctness/useExhaustiveDependencies: `run` is keyed by `key`; a new closure with the same key is the same query.
  useEffect(() => {
    let live = true;
    let pending = cache.get(key) as Promise<T> | undefined;
    if (!pending) {
      pending = run();
      cache.set(key, pending);
      pending.catch(() => cache.delete(key));
    }
    pending.then(
      (data) => live && setQuery({ state: "ready", data }),
      (error: unknown) =>
        live &&
        setQuery({
          state: "error",
          message: error instanceof IpcError ? error.message : String(error),
        }),
    );
    return () => {
      live = false;
    };
  }, [key]);
  return query;
}
