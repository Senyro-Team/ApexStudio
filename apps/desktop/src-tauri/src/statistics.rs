use super::{projects::work, CommandError};
use apex_application::ApplicationServices;
use apex_storage::{SalesSnapshot, StatisticsSnapshot, Workspace};

#[tauri::command]
pub async fn statistics_profile(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<String>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.statistics_profile()).await
}

#[tauri::command]
pub async fn set_statistics_profile(
    profile_id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.set_statistics_profile(profile_id.trim())).await
}

#[tauri::command]
pub async fn statistics_history(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<StatisticsSnapshot>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || match ws.statistics_profile()? {
        Some(id) => ws.statistics_snapshots(&id),
        None => Ok(Vec::new()),
    })
    .await
}

#[tauri::command]
pub async fn statistics_sales(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<SalesSnapshot>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || match ws.statistics_profile()? {
        Some(id) => ws.statistics_sales(&id),
        None => Ok(None),
    })
    .await
}

#[tauri::command]
pub async fn refresh_statistics(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<StatisticsSnapshot>, CommandError> {
    ApplicationServices::new(workspace.inner().clone())
        .refresh_statistics()
        .await
        .map_err(CommandError::from)
}
