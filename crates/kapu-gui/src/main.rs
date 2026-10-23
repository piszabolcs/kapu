//! `kapu-gui`: the KAPU desktop window (M10, D115).
//!
//! `kapu-gui [file.kapu]` opens a Tauri window with the interface from `ui/`.
//! Three commands reach Rust: `kapu` (every read command of [`Session`]),
//! `pick` (the system file dialog) and `open_external` (a web link in the
//! system browser).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;

use kapu_gui::api::{ApiError, Session};
use serde_json::Value;
use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

fn background(e: &tauri::Error) -> ApiError {
    ApiError::gui(format!("background task failed: {e}"))
}

/// Runs a [`Session`] command off the main thread; `open` also names the window.
#[tauri::command]
async fn kapu(
    window: WebviewWindow,
    session: State<'_, Arc<Session>>,
    cmd: String,
    args: Value,
) -> Result<Value, ApiError> {
    let session = Arc::clone(&session);
    let name = cmd.clone();
    let answer = tauri::async_runtime::spawn_blocking(move || session.dispatch(&name, &args))
        .await
        .map_err(|e| background(&e))??;
    if matches!(cmd.as_str(), "open" | "reload")
        && let Some(file) = answer.get("name").and_then(Value::as_str)
    {
        // A failed title change is cosmetic; the file is open either way.
        let _ = window.set_title(&format!("{file} · KAPU"));
    }
    Ok(answer)
}

/// The system "open file" dialog, filtered to `.kapu`; `None` when cancelled.
#[tauri::command]
async fn pick(app: AppHandle) -> Result<Option<String>, ApiError> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("KAPU", &["kapu"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| background(&e))?;
    Ok(picked.map(|p| p.to_string()))
}

/// Opens an `http`, `https` or `mailto` link from a room in the system browser.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri hands command arguments over by value.
fn open_external(url: String) -> Result<(), ApiError> {
    let lower = url.to_ascii_lowercase();
    if !["https://", "http://", "mailto:"]
        .iter()
        .any(|s| lower.starts_with(s))
    {
        return Err(ApiError::gui(format!("not a web link: {url}")));
    }
    open::that_detached(&url).map_err(|e| ApiError::gui(e.to_string()))
}

fn main() -> std::process::ExitCode {
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(Session::new(initial)))
        .invoke_handler(tauri::generate_handler![kapu, pick, open_external])
        .run(tauri::generate_context!());
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kapu-gui: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
