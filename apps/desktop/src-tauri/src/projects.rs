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
    sync: Option<super::team_sync::ProjectSyncChoice>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Project, CommandError> {
    let destination = if let Some(choice) = &sync {
        super::team_sync::validate_sync_choice(choice)
            .await
            .map_err(|message| CommandError {
                code: "sync_configuration".into(),
                message,
            })?
    } else {
        None
    };
    let workspace = workspace.inner().clone();
    work(move || {
        if destination.is_some() && fields.name.trim().chars().count()<2 {
            return Err(Error::InvalidProject("Use a project name with at least two characters to synchronize it.".into()));
        }
        // Check destination changes before saving metadata, so errors cannot create duplicate projects.
        if let (Some(id),Some(destination)) = (id.as_deref(),destination.as_ref()) {
            if let Some(old)=workspace.project_sync(id)? {
                if (old.origin!=destination.origin || old.server_instance!=destination.instance || old.team_id!=destination.team)
                    && (old.enabled || old.received_from_team) {
                    return Err(Error::InvalidContent("Disable synchronization before reconnecting this project to a different team. Projects received from a team cannot be moved.".into()));
                }
            }
        }
        let project=workspace.save_project(id.as_deref(), fields)?;
        if sync.is_some() {
            workspace.configure_project_sync(&project.id,destination.as_ref().map(|d|(d.origin.as_str(),d.instance.as_str(),d.team.as_str())))?;
        }
        Ok(project)
    }).await
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
