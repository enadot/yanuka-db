//! The Tauri shell.
//!
//! Deliberately thin: it opens the database, runs migrations, takes a backup,
//! and marshals arguments between the webview and `yanuka-db`. No SQL and no
//! business logic live here, which is what keeps the interesting code testable
//! on a machine that cannot build a webview.

mod backup;
mod commands;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app
                .path()
                .app_data_dir()
                .expect("the platform must provide an application data directory");
            std::fs::create_dir_all(&directory)?;

            let database_path = directory.join("contacts.db");

            // Taken before migrating, not after. A migration that corrupts data
            // is the one failure this product cannot recover from any other way,
            // and forward-only migrations mean there is no `down` to fall back
            // on. See docs/DATABASE.md.
            if database_path.exists() {
                backup::before_migration(&database_path)?;
            }

            let state = AppState::open(&database_path)?;
            // One backup per day, on launch. A failure here must never keep
            // the user from their data — it is reported, not fatal.
            state.with(|connection| {
                match yanuka_db::backup::daily_backup(connection, &database_path, 7) {
                    Ok(Some(path)) => eprintln!("daily backup: {}", path.display()),
                    Ok(None) => {}
                    Err(error) => eprintln!("daily backup failed: {error}"),
                }
            });
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::search_contacts,
            commands::suggest_contacts,
            commands::list_contacts,
            commands::get_contact,
            commands::recent_contacts,
            commands::favorite_contacts,
            commands::deleted_contacts,
            commands::create_contact,
            commands::quick_add_contact,
            commands::update_contact,
            commands::delete_contact,
            commands::restore_contact,
            commands::set_favorite,
            commands::touch_contact,
            commands::find_duplicates,
            commands::list_duplicate_pairs,
            commands::merge_contacts,
            commands::list_tags,
            commands::create_tag,
            commands::delete_tag,
            commands::list_categories,
            commands::create_category,
            commands::delete_category,
            commands::list_organizations,
            commands::create_organization,
            commands::delete_organization,
            commands::create_relationship,
            commands::delete_relationship,
            commands::add_note,
            commands::update_note,
            commands::delete_note,
            commands::database_stats,
            commands::backup_database,
            commands::backup_status,
            commands::save_exported_csv,
            commands::audit_log,
            commands::sync_status,
            commands::sync_connect,
            commands::sync_now,
            commands::sync_share_code,
            commands::sync_disconnect,
            commands::conflicts_open,
            commands::conflicts_resolve,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the application");
}
