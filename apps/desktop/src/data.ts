import { IpcError } from "@skyla/ipc";
import { useEffect, useState } from "react";

export type Query<T> =
  | { readonly state: "loading" }
  | { readonly state: "error"; readonly message: string }
  | { readonly state: "ready"; readonly data: T };

const cache = new Map<string, Promise<unknown>>();
const listeners = new Set<() => void>();
let generation = 0;

/**
 * Forgets every answer after a write (a draft saved, an invoice issued):
 * one write can move any report, so every mounted query asks the core again.
 */
export function invalidateAll(): void {
  cache.clear();
  generation += 1;
  for (const listener of listeners) listener();
}

/**
 * Runs a core command once per key and shares the answer. The core is the
 * source of truth; the webview only caches what it said until the next write.
 */
export function useQuery<T>(key: string, run: () => Promise<T>): Query<T> {
  const [query, setQuery] = useState<Query<T>>({ state: "loading" });
  const [seen, setSeen] = useState(generation);
  useEffect(() => {
    const listener = () => setSeen(generation);
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `run` is keyed by `key`; a new closure with the same key is the same query, and `seen` changes after a write.
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
  }, [key, seen]);
  return query;
}

/** The problems a failed command listed, one per line. */
export function problems(error: unknown): string[] {
  const text = error instanceof IpcError ? error.failure.message : String(error);
  return text.split("; ").filter((p) => p.length > 0);
}
