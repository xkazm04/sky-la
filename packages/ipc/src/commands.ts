/**
 * The command contract between the webview and the Rust core.
 * Hand-written until WP-09 replaces it with tauri-specta generated bindings;
 * keep every entry in sync with a `#[tauri::command]` in `apps/desktop/src-tauri`.
 */

/** Mirrors `AppInfo` in `apps/desktop/src-tauri/src/lib.rs`. */
export interface AppInfo {
  readonly name: string;
  readonly version: string;
  readonly transport: "tauri" | "mock";
}

export interface CommandMap {
  app_info: { args: undefined; result: AppInfo };
}

export type CommandName = keyof CommandMap;
export type CommandArgs<C extends CommandName> = CommandMap[C]["args"];
export type CommandResult<C extends CommandName> = CommandMap[C]["result"];
