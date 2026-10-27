use apex_core::{WorkspaceInfo, WorkspaceRepository};
use apex_storage::Workspace;
use tauri::Manager;
mod assets;
mod notes;
mod projects;
mod publishing;
mod roblox;
mod upload_queue;

#[derive(serde::Serialize)]
struct CommandError {
    code: String,
    message: String,
}
impl From<apex_core::Error> for CommandError {
    fn from(error: apex_core::Error) -> Self {
        Self {
            code: error.code().into(),
            message: error.to_string(),
        }
    }
}

#[tauri::command]
async fn workspace_info(
    workspace: tauri::State<'_, Workspace>,
) -> Result<WorkspaceInfo, CommandError> {
    let workspace = workspace.inner().clone();
    tauri::async_runtime::spawn_blocking(move || workspace.info())
        .await
        .map_err(|_| CommandError {
            code: "task_failed".into(),
            message: "The workspace task could not finish. Restart Apex Studio and try again."
                .into(),
        })?
        .map_err(Into::into)
}

#[tauri::command]
async fn rename_workspace(
    name: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<WorkspaceInfo, CommandError> {
    let workspace = workspace.inner().clone();
    tauri::async_runtime::spawn_blocking(move || apex_core::rename_workspace(&workspace, &name))
        .await
        .map_err(|_| CommandError {
            code: "task_failed".into(),
            message: "The workspace task could not finish. Restart Apex Studio and try again."
                .into(),
        })?
        .map_err(Into::into)
}

/// Start the desktop adapter with one local workspace and a minimal command surface.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("apex-thumb", |context, request, responder| {
            let ws = context.app_handle().state::<Workspace>().inner().clone();
            let hash = request.uri().path().trim_start_matches('/').to_owned();
            tauri::async_runtime::spawn_blocking(move || {
                let result = ws.thumbnail_bytes(&hash);
                let mut response = tauri::http::Response::new(
                    result.as_ref().map(|v| v.clone()).unwrap_or_default(),
                );
                *response.status_mut() = if result.is_ok() {
                    tauri::http::StatusCode::OK
                } else {
                    tauri::http::StatusCode::NOT_FOUND
                };
                response.headers_mut().insert(
                    "content-type",
                    tauri::http::HeaderValue::from_static("image/png"),
                );
                if result.is_ok() {
                    response.headers_mut().insert(
                        "cache-control",
                        tauri::http::HeaderValue::from_static("max-age=31536000, immutable"),
                    );
                }
                responder.respond(response);
            });
        })
        .plugin(tauri_plugin_dialog::init())
        .manage(roblox::RobloxState::default())
        .manage(notes::PendingArchives::default())
        .manage(assets::LiveryDrafts::default())
        .setup(|app| {
            let root = app.path().app_local_data_dir()?.join("workspace");
            let ws = Workspace::open(root)?;
            ws.recover_queue()?;
            let runtime = upload_queue::UploadRuntime::new()?;
            app.manage(ws.clone());
            app.manage(runtime.clone());
            upload_queue::setup_tray(app.handle(), runtime.clone(), ws.clone())?;
            runtime.start(app.handle().clone(), ws);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(runtime) = window
                    .app_handle()
                    .try_state::<std::sync::Arc<upload_queue::UploadRuntime>>()
                {
                    if runtime.editing.load(std::sync::atomic::Ordering::Relaxed) {
                        api.prevent_close();
                        let _ = tauri::Emitter::emit(window.app_handle(), "close-blocked", ());
                    } else if runtime.is_active() {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            roblox::roblox_settings,
            roblox::save_roblox_settings,
            roblox::test_roblox_connection,
            roblox::clear_roblox_settings,
            roblox::roblox_livery,
            upload_queue::roblox_liveries,
            upload_queue::upload_snapshot,
            upload_queue::preview_upload_batch,
            upload_queue::start_upload_batch,
            upload_queue::control_upload_batch,
            upload_queue::set_edit_lock,
            publishing::publishing_draft,
            publishing::publishing_previews,
            publishing::save_publishing,
            publishing::pick_publishing_images,
            publishing::export_publishing,
            workspace_info,
            rename_workspace,
            projects::list_projects,
            projects::save_project,
            projects::archive_project,
            projects::list_assets,
            projects::import_assets,
            projects::save_asset_copy,
            projects::vehicle_catalogue,
            notes::list_notes,
            notes::create_note,
            notes::save_note,
            notes::archive_note,
            notes::attach_note_image,
            notes::note_image,
            notes::export_project,
            notes::create_backup,
            notes::inspect_archive,
            notes::apply_archive,
            assets::asset_board,
            assets::assign_asset_slot,
            assets::copy_asset_item,
            assets::save_folder,
            assets::move_asset,
            assets::asset_versions,
            assets::restore_asset_version,
            assets::set_folder_cover,
            assets::livery_preparations,
            assets::prepare_livery,
            assets::prepare_livery_folders,
            assets::commit_livery,
            assets::cancel_livery
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("Apex Studio could not start: {error}");
        std::process::exit(1);
    }
}
