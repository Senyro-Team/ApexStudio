use super::CommandError;
use apex_core::{Asset, Catalogue, Error, ImportReport, Project, ProjectInput};
use apex_storage::Workspace;
use tauri_plugin_dialog::DialogExt;

pub(super) async fn work<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| CommandError {
            code: "task_failed".into(),
            message: "The operation could not finish. Refresh and try again.".into(),
        })?
        .map_err(Into::into)
}
#[tauri::command]
pub async fn list_projects(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<Project>, CommandError> {
    let workspace = workspace.inner().clone();
    work(move || workspace.projects()).await
}
#[tauri::command]
pub async fn save_project(
    id: Option<String>,
    fields: ProjectInput,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Project, CommandError> {
    let workspace = workspace.inner().clone();
    work(move || workspace.save_project(id.as_deref(), fields)).await
}
#[tauri::command]
pub async fn archive_project(
    id: String,
    archived: bool,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Project, CommandError> {
    let workspace = workspace.inner().clone();
    work(move || workspace.archive_project(&id, archived)).await
}
#[tauri::command]
pub async fn delete_archived_project(
    id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let workspace = workspace.inner().clone();
    work(move || workspace.delete_archived_project(&id)).await
}
#[tauri::command]
pub async fn list_assets(
    project_id: Option<String>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<Asset>, CommandError> {
    let workspace = workspace.inner().clone();
    work(move || workspace.assets(project_id.as_deref())).await
}
#[tauri::command]
pub fn vehicle_catalogue() -> Result<Catalogue, CommandError> {
    apex_core::vehicle_catalogue().map_err(Into::into)
}
#[tauri::command]
pub async fn import_assets(
    project_id: String,
    folder_id: Option<String>,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<ImportReport>, CommandError> {
    let workspace = workspace.inner().clone();
    work(move || {
        if workspace.project(&project_id)?.archived {
            return Err(Error::Archived);
        }
        let Some(files) = app
            .dialog()
            .file()
            .set_title("Import files into this project")
            .blocking_pick_files()
        else {
            return Ok(None);
        };
        let paths = files
            .into_iter()
            .map(|file| {
                file.into_path()
                    .map_err(|error| Error::Storage(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        workspace
            .import_files_into(&project_id, folder_id.as_deref(), &paths)
            .map(Some)
    })
    .await
}
#[tauri::command]
pub async fn save_asset_copy(
    id: String,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<bool, CommandError> {
    let workspace = workspace.inner().clone();
    work(move || {
        let asset = workspace
            .assets(None)?
            .into_iter()
            .find(|asset| asset.id == id)
            .ok_or(Error::NotFound)?;
        let Some(file) = app
            .dialog()
            .file()
            .set_title("Save a copy (choose a new filename)")
            .set_file_name(&asset.name)
            .blocking_save_file()
        else {
            return Ok(false);
        };
        workspace.save_asset_copy(
            &id,
            &file
                .into_path()
                .map_err(|error| Error::Storage(error.to_string()))?,
        )?;
        Ok(true)
    })
    .await
}
