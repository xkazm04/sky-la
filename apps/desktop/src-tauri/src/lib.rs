//! sky-la desktop shell. Commands here stay thin: they validate input and
//! delegate to the core crates, which own every invariant.

use serde::Serialize;

/// Build information shown in the status line and used as the IPC smoke test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Product name.
    pub name: &'static str,
    /// Desktop shell version.
    pub version: &'static str,
    /// Which IPC transport answered: always `"tauri"` from the Rust side.
    pub transport: &'static str,
}

/// Returns build information. Mirrors `AppInfo` in `packages/ipc`.
#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "sky-la",
        version: env!("CARGO_PKG_VERSION"),
        transport: "tauri",
    }
}

/// Starts the desktop application.
///
/// # Panics
/// If the Tauri runtime fails to start, there is nothing to recover to.
#[allow(clippy::expect_used)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("error while running the sky-la desktop shell");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_serialises_with_the_shape_the_frontend_expects() {
        let json = serde_json::to_value(app_info()).unwrap_or_default();
        assert_eq!(json["name"], "sky-la");
        assert_eq!(json["transport"], "tauri");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    }
}
