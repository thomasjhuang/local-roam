mod commands;
mod folgezettel;
mod index;
mod linker;
mod settings;
mod state;
mod vault;
// capture bundle (#18): each slice declares its module here, append-only.
mod templates;
mod daily;
mod bibtex;
mod clip;
// sources library (#19): the PDF reading layer.
mod sources;

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
            commands::backlinks,
            commands::search,
            commands::list_tags,
            commands::notes_by_tag,
            // capture bundle (#18): each slice appends its command(s) here, append-only.
            commands::list_templates,
            commands::create_from_template,
            commands::open_daily_note,
            commands::import_citation,
            commands::clip_url,
            // sources library (#19): the PDF reading layer.
            commands::list_sources,
            commands::import_pdf_source,
            commands::open_source,
            // v3 card/thread model (#22): read surfaces for the new UI (#23+).
            commands::list_threads,
            commands::thread_cards,
            commands::list_cards,
            commands::card_memberships,
            commands::card_targets,
            // v3 thread view + card editor (#23): write surfaces.
            commands::get_thread,
            commands::save_card,
            commands::rename_thread,
            commands::new_thread,
            commands::add_card,
            commands::split_card,
            commands::merge_card_up,
            commands::card_backlinks,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
