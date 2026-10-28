// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod render;
mod state;
mod synthetic;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::open_surface,
            commands::clear_surfaces,
            commands::prepare_synthetic,
            commands::fetch_staged,
            commands::build_snap_index,
            commands::snap_ray,
            commands::bench_config,
            commands::write_results,
            commands::exit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zovlyn Pit Design");
}
