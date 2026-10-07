import type { CommandArgs, CommandName, CommandResult } from "./commands";
import { IpcError, type Transport } from "./transport";

export type MockHandlers = {
  [C in CommandName]?: (args: CommandArgs<C>) => CommandResult<C> | Promise<CommandResult<C>>;
};

/** Default handlers. WP-09 backs them with the `@skyla/fixtures` dataset. */
export const defaultMockHandlers: MockHandlers = {
  app_info: () => ({ name: "sky-la", version: "0.1.0", transport: "mock" }),
};

/**
 * An in-process transport for running the UI in a plain browser (`dev:web`),
 * in unit tests and in Playwright. It never touches the network.
 */
export function createMockTransport(handlers: MockHandlers = defaultMockHandlers): Transport {
  return {
    kind: "mock",
    async invoke<C extends CommandName>(command: C, args?: CommandArgs<C>) {
      const handler = handlers[command] as
        | ((args: CommandArgs<C>) => CommandResult<C> | Promise<CommandResult<C>>)
        | undefined;
      if (!handler) {
        throw new IpcError(command, "no mock handler registered");
      }
      return handler(args as CommandArgs<C>);
    },
  };
}
