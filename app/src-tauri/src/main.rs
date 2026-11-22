// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod changes;
mod commands;
mod licence;
mod render;
mod section;
mod selection;
mod shared;
mod state;
mod synthetic;

use tauri::{Emitter, Manager, RunEvent, WindowEvent};

fn main() {
    // The licence is read before anything runs (decision 0098): without one, the app is the Viewer.
    licence::check();
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
            commands::set_layer_filter,
            commands::set_reserves_filter,
            commands::reserves_filter_rows,
            commands::layer_attributes,
            commands::workflow_input_rows,
            commands::set_objects_visible,
            commands::set_looks,
            commands::set_layer_opacity,
            commands::set_contours,
            commands::run_log,
            commands::objects_attributes,
            commands::solids_table,
            commands::table_grid,
            commands::reserves_tree,
            commands::reserves_breakdown,
            commands::reserves_solids,
            commands::reserves_field_values,
            commands::reserves_column,
            commands::reserves_column_trace,
            commands::reserves_rerun_params,
            commands::parameters_picked,
            commands::parameters_check,
            commands::dataflow_check,
            commands::formula_check,
            commands::formula_trace,
            commands::formula_reference,
            commands::reserves_node_of,
            commands::deswik_layers,
            commands::spry_contents,
            commands::stage_object,
            commands::stage_levels,
            commands::stage_layer_levels,
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
            commands::snap_rays,
            commands::section_snap,
            commands::section_pick,
            commands::bench_config,
            commands::write_results,
            commands::write_picture,
            commands::project_unsaved,
            commands::exit_app,
            licence::licence_status,
            licence::load_licence,
            licence::command_modules,
        ])
        .on_window_event(|window, event| {
            // Closing with work unsaved, or a run going, asks first (app review brief, 17 Nov, item 1): the window
            // stays and the app is told to ask Save, Don't save or Cancel. Nothing to lose closes at once.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.state::<state::AppState>().app.lock().is_ok_and(|app| app.would_lose_work())
            {
                api.prevent_close();
                let _ = window.emit("close-asked", ());
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Zovlyn Pit Design")
        .run(|handle, event| {
            // However the app closes, the project file is let go of, so its lock note doesn't outlive the window.
            if let RunEvent::Exit = event
                && let Ok(mut app) = handle.state::<state::AppState>().app.lock()
            {
                app.closing();
            }
        });
}
