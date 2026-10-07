/** True inside the Tauri webview, false in a plain browser or under Node. */
export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
