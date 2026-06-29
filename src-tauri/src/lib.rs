mod commands;
mod index;
mod linker;
mod recall;
mod settings;
mod state;
mod vault;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            app.manage(AppState::new(config_dir.join("settings.json")));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_saved_vault,
            commands::open_vault,
            commands::list_notes,
            commands::get_note,
            commands::create_note,
            commands::save_note,
            commands::delete_note,
            commands::resolve_link,
            commands::commit_link,
            commands::outgoing,
            commands::restore_link,
            commands::submit_recall,
            commands::search,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
