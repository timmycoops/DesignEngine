// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod changes;
mod commands;
mod render;
mod section;
mod selection;
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
            commands::run_workflow,
            commands::run_all_cases,
            commands::capture_workflow,
            commands::add_workflow_step,
            commands::write_workflow_report,
            commands::workflow_reports,
            commands::open_workflow_report,
            commands::run_progress,
            commands::cancel_run,
            commands::undo,
            commands::redo,
            commands::new_project,
            commands::open_project,
            commands::save_project,
            commands::check_links,
            commands::set_layer_visible,
            commands::set_objects_visible,
            commands::set_looks,
            commands::set_layer_opacity,
            commands::set_contours,
            commands::run_log,
            commands::objects_attributes,
            commands::deswik_layers,
            commands::spry_contents,
            commands::stage_object,
            commands::stage_layer_solids,
            commands::objects_bounds,
            commands::solids_under,
            commands::check_slice,
            commands::check_cut_by_polygons,
            commands::check_cut_by_seams,
            commands::objects_in_box,
            commands::section_lines,
            commands::stage_seam_horizon,
            commands::block_values,
            commands::prepare_synthetic,
            commands::fetch_mesh_shared,
            commands::fetch_mesh_ipc,
            commands::build_snap_index,
            commands::snap_ray,
            commands::section_snap,
            commands::section_pick,
            commands::bench_config,
            commands::write_results,
            commands::write_picture,
            commands::exit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zovlyn Pit Design");
}
