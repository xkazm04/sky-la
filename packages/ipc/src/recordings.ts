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
  /** The scripted flow this step belongs to; absent for the initial answers. */
  readonly scenario?: string;
  /** The state this answer holds in; absent for the initial state. */
  readonly state?: string;
  /** For a write: the state it leads to. */
  readonly leadsTo?: string;
}

/** Where a mock session is in the recorded scenarios. */
export interface ReplaySession {
  state: string | null;
}

/** Structural equality; a missing key equals `null`, as serde sees it. */
function same(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a === null || b === null || a === undefined || b === undefined) {
    return (a ?? null) === (b ?? null);
  }
  if (Array.isArray(a) || Array.isArray(b)) {
    return (
      Array.isArray(a) &&
      Array.isArray(b) &&
      a.length === b.length &&
      a.every((x, i) => same(x, b[i]))
    );
  }
  if (typeof a === "object" && typeof b === "object") {
    const x = a as Record<string, unknown>;
    const y = b as Record<string, unknown>;
    const keys = new Set([...Object.keys(x), ...Object.keys(y)]);
    return [...keys].every((k) => same(x[k], y[k]));
  }
  return false;
}

function sameArgs(a: Readonly<Record<string, unknown>>, b: Readonly<Record<string, unknown>>) {
  return same(a, b);
}

/**
 * The mock core: answers a command with its recorded result, rejects with the
 * recorded failure, and refuses anything that wasn't recorded. It never
 * computes; every value is what the Rust core returned.
 *
 * Scenarios make it stateful: a recorded write moves the session to the
 * state it led to, and reads answer from that state first, then from the
 * initial answers for everything the scenario didn't touch.
 */
export function replay(
  recordings: readonly Recording[],
  command: string,
  args: unknown,
  session: ReplaySession = { state: null },
): unknown {
  const given = (args ?? {}) as Record<string, unknown>;
  const matches = (r: Recording) => r.command === command && sameArgs(r.args, given);
  const recording =
    recordings.find((r) => matches(r) && (r.state ?? null) === session.state) ??
    (session.state === null
      ? undefined
      : recordings.find((r) => matches(r) && r.scenario === undefined));
  if (!recording) {
    const failure: IpcFailure = {
      code: "not_recorded",
      message: `${command} ${JSON.stringify(given)} isn't in the demo recordings; add it to skyla_app::recordings::canonical_requests`,
    };
    throw failure;
  }
  if (recording.isError) throw recording.result;
  if (recording.leadsTo) session.state = recording.leadsTo;
  return recording.result;
}
