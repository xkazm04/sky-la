import { invoke } from "@tauri-apps/api/core";
import type { CommandArgs, CommandName, CommandResult } from "./commands";
import { IpcError, type Transport } from "./transport";

export function createTauriTransport(): Transport {
  return {
    kind: "tauri",
    async invoke<C extends CommandName>(command: C, args?: CommandArgs<C>) {
      try {
        return await invoke<CommandResult<C>>(command, args ?? {});
      } catch (error) {
        throw new IpcError(command, error instanceof Error ? error.message : String(error));
      }
    },
  };
}

/** True inside the Tauri webview, false in a plain browser or under Node. */
export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
