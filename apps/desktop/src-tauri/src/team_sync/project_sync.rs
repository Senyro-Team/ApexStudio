use apex_application::team_sync::project as service;
use apex_storage::{ProjectSync, Workspace};
use tauri::{Manager, State};

pub use service::{ProjectSyncChoice, SyncDestination, SyncReport};

fn can_apply(app: tauri::AppHandle) -> std::sync::Arc<dyn Fn() -> bool + Send + Sync> {
    std::sync::Arc::new(move || {
        app.try_state::<std::sync::Arc<crate::upload_queue::UploadRuntime>>()
            .is_none_or(|runtime| !runtime.editing.load(std::sync::atomic::Ordering::Relaxed))
    })
}

pub async fn remote_livery_thumbnail(
    workspace: &Workspace,
    project_id: &str,
    hash: &str,
) -> Result<Vec<u8>, String> {
    service::remote_livery_thumbnail(workspace, project_id, hash).await
}

pub async fn validate_sync_choice(
    choice: &ProjectSyncChoice,
) -> Result<Option<SyncDestination>, String> {
    service::validate_sync_choice(choice).await
}

#[tauri::command]
pub async fn project_sync_settings(
    project_id: String,
    workspace: State<'_, Workspace>,
) -> Result<Option<ProjectSync>, String> {
    service::project_sync_settings(&project_id, &workspace).await
}

#[tauri::command]
pub async fn list_project_sync_settings(
    workspace: State<'_, Workspace>,
) -> Result<Vec<ProjectSync>, String> {
    service::list_project_sync_settings(&workspace).await
}

#[tauri::command]
pub async fn set_project_sync(
    project_id: String,
    choice: ProjectSyncChoice,
    state: State<'_, super::TeamSyncState>,
    workspace: State<'_, Workspace>,
) -> Result<Option<ProjectSync>, String> {
    let _guard = state.0.lock().await;
    service::set_project_sync(&project_id, choice, &workspace).await
}

#[tauri::command]
pub async fn reconnect_project_sync(
    project_id: String,
    choice: ProjectSyncChoice,
    workspace: State<'_, Workspace>,
    app: tauri::AppHandle,
) -> Result<Option<ProjectSync>, String> {
    service::reconnect_project_sync_guarded(&project_id, choice, &workspace, can_apply(app)).await
}

#[tauri::command]
pub async fn sync_projects(
    server_url: String,
    team_id: String,
    workspace: State<'_, Workspace>,
    app: tauri::AppHandle,
) -> Result<SyncReport, String> {
    service::sync_projects_guarded(&server_url, &team_id, &workspace, can_apply(app)).await
}

#[tauri::command]
pub async fn resolve_project_sync(
    project_id: String,
    use_server: bool,
    state: State<'_, super::TeamSyncState>,
    workspace: State<'_, Workspace>,
) -> Result<(), String> {
    let _guard = state.0.lock().await;
    service::resolve_project_sync(&project_id, use_server, &workspace).await
}
