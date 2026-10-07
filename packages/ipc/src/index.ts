import { createMockTransport } from "./mock";
import { createTauriTransport, isTauriRuntime } from "./tauri";
import type { Transport } from "./transport";

export type { AppInfo, CommandArgs, CommandMap, CommandName, CommandResult } from "./commands";
export { createMockTransport, defaultMockHandlers, type MockHandlers } from "./mock";
export { createTauriTransport, isTauriRuntime } from "./tauri";
export { IpcError, type Transport } from "./transport";

/**
 * Picks the transport for this runtime: the Rust core inside Tauri, the
 * fixture-backed mock everywhere else. `forceMock` lets `dev:web` and
 * Playwright pin the mock explicitly.
 */
export function selectTransport(options: { forceMock?: boolean } = {}): Transport {
  if (options.forceMock || !isTauriRuntime()) {
    return createMockTransport();
  }
  return createTauriTransport();
}
