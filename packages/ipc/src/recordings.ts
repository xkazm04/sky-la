import type { IpcFailure } from "./bindings";

/**
 * One request and the core's answer, as written by `skyla_app::recordings`
 * into `packages/fixtures/data/ipc-recordings.json`.
 */
export interface Recording {
  readonly command: string;
  readonly args: Readonly<Record<string, unknown>>;
  readonly result: unknown;
  readonly isError: boolean;
}

function sameArgs(a: Readonly<Record<string, unknown>>, b: Readonly<Record<string, unknown>>) {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
  return [...keys].every((k) => (a[k] ?? null) === (b[k] ?? null));
}

/**
 * The mock core: answers a command with its recorded result, rejects with the
 * recorded failure, and refuses anything that wasn't recorded. It never
 * computes; every value is what the Rust core returned.
 */
export function replay(recordings: readonly Recording[], command: string, args: unknown): unknown {
  const given = (args ?? {}) as Record<string, unknown>;
  const recording = recordings.find((r) => r.command === command && sameArgs(r.args, given));
  if (!recording) {
    const failure: IpcFailure = {
      code: "not_recorded",
      message: `${command} ${JSON.stringify(given)} isn't in the demo recordings; add it to skyla_app::recordings::canonical_requests`,
    };
    throw failure;
  }
  if (recording.isError) throw recording.result;
  return recording.result;
}
