use apex_core::{WorkspaceInfo, WorkspaceRepository};
use apex_storage::Workspace;
use tauri::Manager;
mod assets;
mod notes;
mod projects;
mod publishing;

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
        .plugin(tauri_plugin_dialog::init())
        .manage(notes::PendingArchives::default())
        .manage(assets::LiveryDrafts::default())
        .setup(|app| {
            let root = app.path().app_local_data_dir()?.join("workspace");
            app.manage(Workspace::open(root)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            assets::prepare_livery,
            assets::commit_livery,
            assets::cancel_livery
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("Apex Studio could not start: {error}");
        std::process::exit(1);
    }
}
