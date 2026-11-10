use tauri::State;
use tokio::sync::Mutex;

pub use apex_application::team_sync::{
    ConnectionStatus as SyncConnectionStatus, Invitation as SyncInvitation,
    ServerInfo as SyncServerInfo, TeamMember as SyncTeamMember,
};

mod project_sync;
pub use project_sync::*;

#[derive(Default)]
pub struct TeamSyncState(pub Mutex<()>);

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
