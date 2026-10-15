mod commands;
mod dto;
mod sandbox;

use commands::{grant_dropped, AppState, DROPPED_EVENT};
use tauri::Emitter;
use tauri::Manager;

pub use commands::{
    accepted_dropped_paths, create_archive_direct, extract_archive_direct, list_archive_direct,
    plan_archive_direct, PROGRESS_EVENT,
};
pub use dto::{CreateDto, ExtractDto, ListReportDto, PlanDto, WriteReportDto};
pub use sandbox::Sandbox;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::plan_archive,
            commands::create_archive,
            commands::list_archive,
            commands::extract_archive,
            commands::cancel_job,
            commands::pick_files,
            commands::pick_save_zip,
            commands::pick_extract_dir,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                let Some(state) = window.try_state::<AppState>() else {
                    return;
                };
                let granted = grant_dropped(&state, window.app_handle(), paths);
                let _ = window.emit(DROPPED_EVENT, &granted);
            }
        })
        .run(tauri::generate_context!())
        .expect("アプリの起動に失敗しました");
}
