import recordings from "@skyla/fixtures/ipc-recordings.json";
import { mockIPC } from "@tauri-apps/api/mocks";
import type { IpcFailure } from "./bindings";
import { type Recording, replay } from "./recordings";
import { isTauriRuntime } from "./tauri";

export * from "./bindings";
export { type Recording, replay } from "./recordings";
export { isTauriRuntime } from "./tauri";

/** Which core answers: the Rust core inside Tauri, or the recorded one. */
export type CoreKind = "tauri" | "mock";

/**
 * Connects the generated `commands` to a core. Inside the Tauri webview they
 * reach the Rust core; anywhere else (`dev:web`, unit tests, Playwright) the
 * IPC layer is mocked to replay the core's recordings, so the same bindings
 * run unchanged and return exactly what the Rust core returned.
 */
export function connectCore(
  options: { forceMock?: boolean; recordings?: readonly Recording[] } = {},
): CoreKind {
  if (!options.forceMock && isTauriRuntime()) return "tauri";
  const source = options.recordings ?? (recordings as readonly Recording[]);
  mockIPC((command, args) => replay(source, command, args));
  return "mock";
}

/** Raised by {@link unwrap} for a failed command. */
export class IpcError extends Error {
  constructor(readonly failure: IpcFailure) {
    super(`${failure.code}: ${failure.message}`);
    this.name = "IpcError";
  }
}

/** Returns a command's data, or throws its failure as an {@link IpcError}. */
export async function unwrap<T>(
  result: Promise<{ status: "ok"; data: T } | { status: "error"; error: IpcFailure }>,
): Promise<T> {
  const settled = await result;
  if (settled.status === "error") throw new IpcError(settled.error);
  return settled.data;
}
