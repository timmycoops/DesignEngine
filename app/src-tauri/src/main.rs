// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod render;
mod section;
mod shared;
mod state;
mod synthetic;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::project_summary,
            commands::object_details,
            commands::run_command,
            commands::undo,
            commands::redo,
            commands::new_project,
            commands::open_project,
            commands::save_project,
            commands::set_layer_visible,
            commands::run_log,
            commands::stage_object,
            commands::stage_layer_solids,
            commands::section_lines,
            commands::stage_seam_horizon,
            commands::prepare_synthetic,
            commands::fetch_mesh_shared,
            commands::fetch_mesh_ipc,
            commands::build_snap_index,
            commands::snap_ray,
            commands::section_snap,
            commands::bench_config,
            commands::write_results,
            commands::exit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zovlyn Pit Design");
}
