//! WP-09 acceptance: every recorded command round-trips identically over the
//! real Tauri IPC and over the webview's mock transport (which replays the
//! same recordings), and the generated bindings are current.

use std::path::PathBuf;

use skyla_app::Core;
use skyla_app::recordings::{RECORDINGS_PATH, Recording};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn app() -> (tauri::App<MockRuntime>, tauri::WebviewWindow<MockRuntime>) {
    // The same reproducible gate the recordings were made with, in its own folder.
    let dir = std::env::temp_dir().join(format!(
        "skyla-ipc-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let demo = Core::demo().expect("demo core");
    let session = skyla_desktop_lib::Session::new(skyla_app::session::Gate::reproducible(dir));
    // The demo, with its recorded provider; books opened in a scenario don't
    // replace it (only one set of books is open at a time).
    session.hold(demo);
    let app = mock_builder()
        .manage(session)
        .invoke_handler(skyla_desktop_lib::specta_builder::<MockRuntime>().invoke_handler())
        .build(mock_context(noop_assets()))
        .expect("app");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("webview");
    (app, webview)
}

fn replay(webview: &tauri::WebviewWindow<MockRuntime>, recording: &Recording) {
    let response = get_ipc_response(
        webview,
        InvokeRequest {
            cmd: recording.command.clone(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("url"),
            body: InvokeBody::Json(recording.args.clone()),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    );
    let (value, is_error) = match response {
        Ok(body) => (
            body.deserialize::<serde_json::Value>().expect("json"),
            false,
        ),
        Err(error) => (error, true),
    };
    assert_eq!(
        is_error, recording.is_error,
        "{} {}",
        recording.command, recording.args
    );
    assert!(
        value == recording.result,
        "{} {} (scenario {:?}) differs over Tauri IPC from the recording",
        recording.command,
        recording.args,
        recording.scenario
    );
}

#[test]
fn every_recording_round_trips_identically_over_tauri_ipc() {
    let recordings: Vec<Recording> = serde_json::from_str(
        &std::fs::read_to_string(repo().join(RECORDINGS_PATH)).expect("recordings"),
    )
    .expect("parse recordings");
    assert!(recordings.len() > 20);
    let (_app, webview) = app();
    for recording in recordings.iter().filter(|r| r.scenario.is_none()) {
        replay(&webview, recording);
    }
    // Each scenario runs in order on its own fresh core, as it was recorded.
    let mut names: Vec<&str> = recordings
        .iter()
        .filter_map(|r| r.scenario.as_deref())
        .collect();
    names.dedup();
    assert!(!names.is_empty());
    for name in names {
        let (_app, webview) = app();
        for recording in recordings
            .iter()
            .filter(|r| r.scenario.as_deref() == Some(name))
        {
            replay(&webview, recording);
        }
    }
}

#[test]
fn a_bad_request_comes_back_as_a_typed_failure() {
    let session = skyla_desktop_lib::Session::new(skyla_app::session::Gate::reproducible(
        std::env::temp_dir().join(format!("skyla-ipc-bad-{}", std::process::id())),
    ));
    session.hold(Core::demo().expect("demo core"));
    let app = mock_builder()
        .manage(session)
        .invoke_handler(skyla_desktop_lib::specta_builder::<MockRuntime>().invoke_handler())
        .build(mock_context(noop_assets()))
        .expect("app");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("webview");
    let response = get_ipc_response(
        &webview,
        InvokeRequest {
            cmd: "profit_and_loss".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("url"),
            body: InvokeBody::Json(serde_json::json!({ "from": "2026-13-01", "to": "2026-09-30" })),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    );
    let failure = response.expect_err("an invalid date is refused");
    assert_eq!(failure["code"], "ledger");
    assert!(
        failure["message"]
            .as_str()
            .unwrap_or_default()
            .contains("2026-13-01")
    );
}

fn call(
    webview: &tauri::WebviewWindow<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, serde_json::Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("url"),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|r| r.deserialize::<serde_json::Value>().expect("json"))
}

#[test]
fn locked_books_answer_nothing_until_unlocked_again() {
    let dir = std::env::temp_dir().join(format!("skyla-ipc-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let gate = skyla_app::session::Gate::reproducible(dir.clone());
    let pass = skyla_app::recordings::FIRST_RUN_PASSPHRASE;
    let (core, _) = gate
        .create(
            &serde_json::from_value(skyla_app::recordings::first_run_setup()).expect("setup"),
            pass,
        )
        .expect("books");
    drop(core);
    let session = skyla_desktop_lib::Session::new(skyla_app::session::Gate::reproducible(dir));
    let app = mock_builder()
        .manage(session)
        .invoke_handler(skyla_desktop_lib::specta_builder::<MockRuntime>().invoke_handler())
        .build(mock_context(noop_assets()))
        .expect("app");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("webview");

    let locked = call(&webview, "invoices", serde_json::json!({})).expect_err("no books yet");
    assert_eq!(locked["code"], "locked");
    let state = call(
        &webview,
        "unlock",
        serde_json::json!({ "passphrase": pass, "remember": false }),
    )
    .expect("unlocks");
    assert_eq!(state["state"], "open");
    assert!(call(&webview, "invoices", serde_json::json!({})).is_ok());

    let state = call(&webview, "lock", serde_json::json!({})).expect("locks");
    assert_eq!(state["state"], "locked");
    let locked = call(&webview, "invoices", serde_json::json!({})).expect_err("locked again");
    assert_eq!(locked["code"], "locked");
    let wrong = call(
        &webview,
        "unlock",
        serde_json::json!({ "passphrase": "not it", "remember": false }),
    );
    assert!(wrong.is_err());
    assert!(
        call(
            &webview,
            "unlock",
            serde_json::json!({ "passphrase": pass, "remember": false })
        )
        .is_ok()
    );
    assert!(call(&webview, "invoices", serde_json::json!({})).is_ok());
}

#[test]
fn the_demo_doesnt_lock() {
    let session = skyla_desktop_lib::Session::new(skyla_app::session::Gate::reproducible(
        std::env::temp_dir().join(format!("skyla-ipc-demo-{}", std::process::id())),
    ));
    session.hold(Core::demo().expect("demo core"));
    assert!(session.lock().is_err());
    assert!(session.books().is_some());
}

#[test]
fn the_generated_bindings_are_current() {
    let committed = repo().join("packages/ipc/src/bindings.ts");
    if std::env::var_os("UPDATE_BINDINGS").is_some() {
        skyla_desktop_lib::export_bindings(&committed).expect("export");
        return;
    }
    let fresh = std::env::temp_dir().join(format!("skyla-bindings-{}.ts", std::process::id()));
    skyla_desktop_lib::export_bindings(&fresh).expect("export");
    let (a, b) = (
        std::fs::read_to_string(&fresh).expect("fresh"),
        std::fs::read_to_string(&committed).unwrap_or_default(),
    );
    let _ = std::fs::remove_file(&fresh);
    assert!(
        a == b,
        "packages/ipc/src/bindings.ts is stale; regenerate it with `just bindings`"
    );
}

#[test]
fn the_window_shows_only_the_apps_own_pages() {
    use skyla_desktop_lib::navigation_allowed;
    let ok = |u: &str| navigation_allowed(&u.parse().unwrap());
    assert!(ok("tauri://localhost/index.html#/invoices"));
    assert!(ok("http://tauri.localhost/#/settings"));
    assert!(ok("https://tauri.localhost/"));
    for remote in [
        "https://www.zakonyprolidi.cz/cs/1992-586",
        "https://tauri.localhost.evil.example/",
        "http://localhost:8080/",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "data:text/html,<h1>Unlock</h1>",
    ] {
        assert!(!ok(remote), "{remote}");
    }
    // The dev server, in debug builds only.
    assert_eq!(ok("http://localhost:1420/"), cfg!(debug_assertions));
}
