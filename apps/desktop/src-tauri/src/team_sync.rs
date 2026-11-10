use tauri::State;
use tokio::sync::Mutex;

pub use apex_application::team_sync::{
    ConnectionStatus as SyncConnectionStatus, HostedImages as SyncHostedImages,
    Invitation as SyncInvitation, ServerInfo as SyncServerInfo,
    ServiceAvailability as SyncServiceAvailability, TeamMember as SyncTeamMember,
};

mod project_sync;
pub use project_sync::*;

#[derive(Default)]
pub struct TeamSyncState(pub Mutex<()>);

#[tauri::command]
pub async fn sync_service_availability(
    server_url: String,
    team_id: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncServiceAvailability, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::service_availability(&server_url, &team_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_image_library(
    server_url: String,
    team_id: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncHostedImages, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::list_hosted_images(&server_url, &team_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_upload_library_image(
    server_url: String,
    team_id: String,
    item_id: String,
    scope: String,
    state: State<'_, TeamSyncState>,
    workspace: State<'_, apex_storage::Workspace>,
) -> Result<apex_storage::MediaLibraryItem, String> {
    let _guard = state.0.lock().await;
    let ws = workspace.inner().clone();
    let read_id = item_id.clone();
    let (name, bytes) = super::projects::work(move || ws.media_library_upload_bytes(&read_id))
        .await
        .map_err(|error| error.message)?;
    let uploaded = apex_application::team_sync::upload_hosted_images(
        &server_url,
        &team_id,
        &scope,
        vec![(name, bytes)],
    )
    .await
    .map_err(|error| error.to_string())?;
    let image = uploaded.images.first().ok_or_else(|| {
        uploaded
            .errors
            .first()
            .and_then(|value| value.get("error"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("The image provider did not return an image.")
            .to_owned()
    })?;
    let ws = workspace.inner().clone();
    let image_id = item_id;
    let provider_id = image.id.clone();
    let image_url = image.image_url.clone();
    let owner_scope = image.owner_scope.clone();
    let team = (owner_scope == "team").then_some(team_id);
    super::projects::work(move || {
        ws.set_media_library_remote(
            &image_id,
            &provider_id,
            &image_url,
            &owner_scope,
            team.as_deref(),
        )
    })
    .await
    .map_err(|error| error.message)
}

#[tauri::command]
pub async fn sync_set_library_image_scope(
    server_url: String,
    team_id: String,
    item_id: String,
    scope: String,
    state: State<'_, TeamSyncState>,
    workspace: State<'_, apex_storage::Workspace>,
) -> Result<apex_storage::MediaLibraryItem, String> {
    let _guard = state.0.lock().await;
    let ws = workspace.inner().clone();
    let item = super::projects::work({
        let ws = ws.clone();
        let item_id = item_id.clone();
        move || ws.media_library_item_by_id(&item_id)
    })
    .await
    .map_err(|error| error.message)?;
    let provider_id = item
        .provider_image_id
        .ok_or("Upload this image before changing its sharing.")?;
    apex_application::team_sync::set_hosted_image_scope(
        &server_url,
        &team_id,
        &provider_id,
        &scope,
    )
    .await
    .map_err(|error| error.to_string())?;
    let new_team_id = (scope == "team").then_some(team_id);
    super::projects::work(move || {
        ws.set_media_library_remote(
            &item_id,
            &provider_id,
            &item.image_url,
            &scope,
            new_team_id.as_deref(),
        )
    })
    .await
    .map_err(|error| error.message)
}

#[tauri::command]
pub async fn sync_check_server(
    server_url: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncServerInfo, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::check_server(&server_url)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_connection_status(
    server_url: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncConnectionStatus, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::connection_status(&server_url)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_sign_in(
    server_url: String,
    email: String,
    password: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncConnectionStatus, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::sign_in(&server_url, &email, &password)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_accept_invitation(
    server_url: String,
    invitation_token: String,
    email: String,
    display_name: String,
    password: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncConnectionStatus, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::accept_invitation(
        &server_url,
        &invitation_token,
        &email,
        &display_name,
        &password,
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_create_invitation(
    server_url: String,
    team_id: String,
    role: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncInvitation, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::create_invitation(&server_url, &team_id, &role)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_list_members(
    server_url: String,
    team_id: String,
    state: State<'_, TeamSyncState>,
) -> Result<Vec<SyncTeamMember>, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::list_members(&server_url, &team_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_remove_member(
    server_url: String,
    team_id: String,
    user_id: String,
    state: State<'_, TeamSyncState>,
) -> Result<(), String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::remove_member(&server_url, &team_id, &user_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_initial_setup(
    server_url: String,
    bootstrap_token: String,
    team_name: String,
    email: String,
    display_name: String,
    password: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncConnectionStatus, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::initial_setup(
        &server_url,
        &bootstrap_token,
        &team_name,
        &email,
        &display_name,
        &password,
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_disconnect(state: State<'_, TeamSyncState>) -> Result<(), String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::disconnect()
        .await
        .map_err(|error| error.to_string())
}
