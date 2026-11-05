#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tauri::Manager;

/// macOS: the default app menu, except that File holds New Tab (⌘T) and Close Tab (⌘W), and
/// Window has no Close Window, so ⌘W always means Close Tab. Both go to the webview as a `menu`
/// event; with one tab left, Close Tab closes the window there. Windows has no app menu; the
/// webview handles Ctrl+T and Ctrl+W itself.
#[cfg(target_os = "macos")]
fn app_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{
        AboutMetadata, HELP_SUBMENU_ID, Menu, MenuItem, PredefinedMenuItem as P, Submenu,
        WINDOW_SUBMENU_ID,
    };
    let info = app.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        ..Default::default()
    };
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                info.name.clone(),
                true,
                &[
                    &P::about(app, None, Some(about))?,
                    &P::separator(app)?,
                    &P::services(app, None)?,
                    &P::separator(app)?,
                    &P::hide(app, None)?,
                    &P::hide_others(app, None)?,
                    &P::separator(app)?,
                    &P::quit(app, None)?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "File",
                true,
                &[
                    &MenuItem::with_id(app, "new-tab", "New Tab", true, Some("CmdOrCtrl+T"))?,
                    &MenuItem::with_id(app, "close-tab", "Close Tab", true, Some("CmdOrCtrl+W"))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &P::undo(app, None)?,
                    &P::redo(app, None)?,
                    &P::separator(app)?,
                    &P::cut(app, None)?,
                    &P::copy(app, None)?,
                    &P::paste(app, None)?,
                    &P::select_all(app, None)?,
                ],
            )?,
            &Submenu::with_items(app, "View", true, &[&P::fullscreen(app, None)?])?,
            &Submenu::with_id_and_items(
                app,
                WINDOW_SUBMENU_ID,
                "Window",
                true,
                &[&P::minimize(app, None)?, &P::maximize(app, None)?],
            )?,
            &Submenu::with_id_and_items(app, HELP_SUBMENU_ID, "Help", true, &[])?,
        ],
    )
}

fn main() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu).on_menu_event(|app, event| {
        use tauri::Emitter;
        let id = event.id().as_ref();
        if id == "new-tab" || id == "close-tab" {
            let _ = app.emit("menu", id);
        }
    });
    builder
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
            openpaper::commands::library_set_read,
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
            openpaper::commands::llm_open_key_page,
            openpaper::commands::llm_backup,
            openpaper::commands::paper_roadmap,
            openpaper::commands::gloss_selection,
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
