mod android_native;
mod cheats;
mod cheatslips;
mod db;
mod games;
mod managed_library;
mod package_library;
mod package_library_pc;
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
mod package_metadata;
mod settings;

use simplelog::{
    ColorChoice, CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode, WriteLogger,
};
use std::fs::OpenOptions;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // ── Logging setup ───────────────────────────────────────────
            // Log to terminal and to a size-capped rotating log file.
            // If the log exceeds 5 MB, rotate it to .old before opening.
            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            std::fs::create_dir_all(&log_dir).ok();
            let log_file = log_dir.join("eden-cheats-manager.log");
            const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
            if log_file.exists() {
                if std::fs::metadata(&log_file).map(|m| m.len()).unwrap_or(0) > MAX_LOG_BYTES {
                    let old = log_dir.join("eden-cheats-manager.log.old");
                    let _ = std::fs::rename(&log_file, &old);
                }
            }
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file)
                .expect("could not open log file");
            CombinedLogger::init(vec![
                TermLogger::new(
                    LevelFilter::Debug,
                    Config::default(),
                    TerminalMode::Mixed,
                    ColorChoice::Auto,
                ),
                WriteLogger::new(LevelFilter::Debug, Config::default(), file),
            ])
            .ok();
            log::info!(
                "Eden Cheats Manager started — log file: {}",
                log_file.display()
            );
            // ── DB init ─────────────────────────────────────────────────
            db::init_db(&app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // settings
            settings::get_settings,
            settings::save_settings,
            settings::detect_pc_load_dir,
            settings::get_app_log_path,
            settings::get_platform,
            // local cheats lookup + custom cheats
            cheatslips::search_cheats,
            cheatslips::save_custom_cheat,
            cheatslips::delete_custom_cheat,
            cheatslips::clear_api_cheats,
            cheatslips::fetch_cheats_online,
            // games
            games::scan_eden_games_pc,
            games::get_cached_games_pc,
            games::get_cached_games_android,
            managed_library::scan_managed_library_pc,
            managed_library::scan_managed_library_android_native,
            // cheats
            cheats::install_cheat_pc,
            cheats::list_installed_cheats_pc,
            cheats::delete_cheat_pc,
            // desktop package library
            package_library_pc::scan_game_package_library_pc,
            package_library_pc::discover_package_metadata_for_title_pc,
            // android native (SAF — no ADB)
            android_native::get_eden_load_access_status,
            android_native::select_eden_load_directory,
            android_native::test_eden_load_directory,
            android_native::get_package_discovery_status,
            android_native::select_prod_keys_document,
            android_native::select_game_package_document,
            android_native::discover_package_metadata,
            android_native::discover_package_metadata_for_title,
            android_native::select_game_library_directory,
            android_native::get_game_library_status,
            android_native::scan_game_package_library,
            android_native::scan_eden_games_android_native,
            android_native::install_cheat_android_native,
            android_native::list_installed_cheats_android_native,
            android_native::delete_cheat_android_native,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
