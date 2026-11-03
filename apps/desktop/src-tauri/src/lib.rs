use apex_core::{WorkspaceInfo, WorkspaceRepository};
use apex_storage::Workspace;
use tauri::Manager;
mod assets;
mod browser_bridge;
#[allow(dead_code)] // Shared framing module is also compiled into the standalone Chrome host.
mod chrome_protocol;
mod notes;
mod posts;
mod projects;
mod publishing;
mod roblox;
mod statistics;
mod team_sync;
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

#[tauri::command]
fn set_app_theme_icon(app: tauri::AppHandle, color: &str) -> Result<(), String> {
    let hex = color.strip_prefix('#').ok_or("Invalid theme color")?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid theme color".into());
    }
    let parse = |start| {
        u8::from_str_radix(&hex[start..start + 2], 16)
            .map_err(|_| "Invalid theme color".to_string())
    };
    let channels = [parse(0)?, parse(2)?, parse(4)?];
    let base =
        tauri::image::Image::from_bytes(include_bytes!("../../public/brand/themes/violet.png"))
            .map_err(|error| error.to_string())?;
    let mut rgba = base.rgba().to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel[..3].copy_from_slice(&channels);
    }
    let icon = tauri::image::Image::new_owned(rgba, base.width(), base.height());
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_icon(icon.clone())
            .map_err(|error| error.to_string())?;
    }
    if let Some(tray) = app.tray_by_id("apex-uploads") {
        tray.set_icon(Some(icon))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
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
        .manage(team_sync::TeamSyncState::default())
        .manage(notes::PendingArchives::default())
        .manage(assets::LiveryDrafts::default())
        .setup(|app| {
            let root = app.path().app_local_data_dir()?.join("workspace");
            let ws = Workspace::open(root)?;
            ws.recover_queue()?;
            let runtime = upload_queue::UploadRuntime::new()?;
            browser_bridge::setup(app.handle(), ws.clone())?;
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
            browser_bridge::chrome_settings,
            browser_bridge::save_chrome_settings,
            browser_bridge::browser_status,
            browser_bridge::check_browser,
            browser_bridge::prepare_browser,
            browser_bridge::control_browser,
            posts::list_posts,
            posts::create_post,
            posts::post_draft,
            posts::save_post,
            posts::post_sources,
            posts::pick_post_images,
            posts::pin_post_images,
            posts::release_post_images,
            posts::export_post,
            publishing::publishing_draft,
            publishing::publishing_previews,
            publishing::save_publishing,
            publishing::pick_publishing_images,
            publishing::export_publishing,
            workspace_info,
            rename_workspace,
            statistics::statistics_profile,
            statistics::set_statistics_profile,
            statistics::statistics_history,
            statistics::refresh_statistics,
            statistics::statistics_sales,
            browser_bridge::start_statistics_browser,
            browser_bridge::statistics_browser_status,
            team_sync::sync_check_server,
            team_sync::sync_connection_status,
            team_sync::sync_sign_in,
            team_sync::sync_accept_invitation,
            team_sync::sync_create_invitation,
            team_sync::sync_initial_setup,
            team_sync::sync_disconnect,
            team_sync::project_sync_settings,
            team_sync::set_project_sync,
            team_sync::sync_projects,
            team_sync::resolve_project_sync,
            set_app_theme_icon,
            projects::list_projects,
            projects::save_project,
            projects::archive_project,
            projects::delete_archived_project,
            projects::list_assets,
            projects::import_assets,
            projects::save_asset_copy,
            projects::vehicle_catalogue,
            notes::list_notes,
            notes::create_note,
            notes::save_note,
            notes::archive_note,
            notes::delete_archived_note,
            notes::attach_note_image,
            notes::note_image,
            notes::export_project,
            notes::create_backup,
            notes::inspect_archive,
            notes::apply_archive,
            assets::trash_preview,
            assets::trash_item,
            assets::list_trash,
            assets::restore_trash,
            assets::purge_trash,
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
