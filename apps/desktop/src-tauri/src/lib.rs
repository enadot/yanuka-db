//! The Tauri shell.
//!
//! Deliberately thin: it opens the database, runs migrations, takes a backup,
//! and marshals arguments between the webview and `yanuka-db`. No SQL and no
//! business logic live here, which is what keeps the interesting code testable
//! on a machine that cannot build a webview.

mod backup;
mod commands;
mod keys;
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
            // the user from their data — it is reported, not fatal. Keyed
            // with the database's own key, so the copies are as protected as
            // the original. Skipped while locked, by construction.
            let key = state.key_pragma();
            let _ = state.with(|connection| {
                match yanuka_db::backup::daily_backup(connection, &database_path, 7, key.as_deref())
                {
                    Ok(Some(path)) => eprintln!("daily backup: {}", path.display()),
                    Ok(None) => {}
                    Err(error) => eprintln!("daily backup failed: {error}"),
                }
                Ok(())
            });
            app.manage(state);

            // Semantic search (ADR-036). The model ships as a bundled
            // resource; when it is absent — a dev run without the fetch
            // script — everything else works and settings says so. After a
            // successful load, a background task reconciles the semantic
            // index in small budgeted steps so the first launch after an
            // upgrade indexes the whole archive without ever holding the
            // database lock long enough for the UI to notice.
            let resources =
                app.path().resolve("resources/semantic", tauri::path::BaseDirectory::Resource);
            if let Ok(directory) = resources {
                match yanuka_db::semantic::SemanticEngine::load(
                    &directory.join("model.onnx"),
                    &directory.join("tokenizer.json"),
                ) {
                    Ok(engine) => {
                        let state = app.state::<AppState>();
                        state.attach_semantic(engine);
                        let handle = app.handle().clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            let state = handle.state::<AppState>();
                            let Some(engine) = state.semantic_engine() else { return };
                            let mut indexed = 0usize;
                            loop {
                                let step = state.with(|connection| {
                                    yanuka_db::semantic::sync_step(connection, &engine, 12)
                                });
                                match step {
                                    Ok(outcome) => {
                                        indexed += outcome.embedded;
                                        state.set_semantic_progress(
                                            indexed,
                                            outcome.pending,
                                            outcome.pending > 0,
                                        );
                                        if outcome.pending == 0 {
                                            break;
                                        }
                                    }
                                    Err(error) => {
                                        eprintln!("semantic catch-up stopped: {error}");
                                        state.set_semantic_progress(indexed, 0, false);
                                        break;
                                    }
                                }
                            }
                        });
                    }
                    Err(error) => eprintln!("semantic engine unavailable: {error}"),
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::search_contacts,
            commands::suggest_contacts,
            commands::list_contacts,
            commands::get_contact,
            commands::recent_contacts,
            commands::favorite_contacts,
            commands::create_contact,
            commands::quick_add_contact,
            commands::update_contact,
            commands::delete_contact,
            commands::restore_contact,
            commands::list_deleted_contacts,
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
            commands::security_status,
            commands::semantic_status,
            commands::recovery_key,
            commands::unlock_database,
            commands::save_exported_csv,
            commands::audit_log,
            commands::ocr_import_page,
            commands::ocr_list_pages,
            commands::ocr_get_page,
            commands::ocr_set_token_text,
            commands::ocr_lexicon,
            commands::ocr_save_note,
            commands::ocr_delete_page,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the application");
}
