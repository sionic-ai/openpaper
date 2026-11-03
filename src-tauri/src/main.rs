#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            // Moves the library of the app's former name; a failure must not stop startup.
            if let Err(e) = openpaper::library::migrate_legacy(&data) {
                eprintln!("could not move the old library: {e}");
            }
            app.manage(Arc::new(openpaper::AppState::new(data.join("papers"))?));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            openpaper::commands::languages,
            openpaper::commands::library_list,
            openpaper::commands::library_get,
            openpaper::commands::library_remove,
            openpaper::commands::library_folders,
            openpaper::commands::library_set_folders,
            openpaper::commands::open_arxiv,
            openpaper::commands::open_pdf,
            openpaper::commands::arxiv_search,
            openpaper::commands::ai_recommend,
            openpaper::commands::llm_status,
            openpaper::commands::llm_settings,
            openpaper::commands::llm_set_settings,
            openpaper::commands::llm_set_key,
            openpaper::commands::llm_models,
            openpaper::commands::llm_test,
            openpaper::commands::open_arxiv_page,
            openpaper::commands::translation_start,
            openpaper::commands::translation_redo,
            openpaper::commands::translation_status,
            openpaper::commands::translation_cancel,
            openpaper::commands::update_check,
            openpaper::commands::update_install,
            openpaper::commands::ask_claude,
            openpaper::commands::summarize_paper,
        ])
        .build(tauri::generate_context!())
        .expect("openpaper failed to start")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<Arc<openpaper::AppState>>().cancel_all();
            }
        });
}
