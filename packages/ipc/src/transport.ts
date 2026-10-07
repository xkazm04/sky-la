import type { CommandArgs, CommandName, CommandResult } from "./commands";

export interface Transport {
  readonly kind: "tauri" | "mock";
  invoke<C extends CommandName>(command: C, args?: CommandArgs<C>): Promise<CommandResult<C>>;
}

/** Raised for any failed command, whichever transport ran it. */
export class IpcError extends Error {
  constructor(
    readonly command: string,
    message: string,
  ) {
    super(`${command}: ${message}`);
    this.name = "IpcError";
  }
}
