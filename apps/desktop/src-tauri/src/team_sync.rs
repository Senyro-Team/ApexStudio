use tauri::State;
use tokio::sync::Mutex;

pub use apex_application::team_sync::{
    ConnectionStatus as SyncConnectionStatus, HostedImages as SyncHostedImages,
    Invitation as SyncInvitation, InviteCampaign as SyncInviteCampaign,
    InviteCampaignInput as SyncInviteCampaignInput, ServerInfo as SyncServerInfo,
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
pub async fn sync_list_invite_campaigns(
    server_url: String,
    team_id: String,
    state: State<'_, TeamSyncState>,
) -> Result<Vec<SyncInviteCampaign>, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::list_invite_campaigns(&server_url, &team_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_create_invite_campaign(
    server_url: String,
    team_id: String,
    request_id: String,
    input: SyncInviteCampaignInput,
    state: State<'_, TeamSyncState>,
) -> Result<SyncInviteCampaign, String> {
    let _guard = state.0.lock().await;
    apex_application::team_sync::create_invite_campaign(&server_url, &team_id, &request_id, &input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_upload_library_images(
    server_url: String,
    team_id: String,
    item_ids: Vec<String>,
    scope: String,
    state: State<'_, TeamSyncState>,
    workspace: State<'_, apex_storage::Workspace>,
) -> Result<SyncHostedImages, String> {
    if item_ids.is_empty()
        || item_ids.len() > 10
        || item_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != item_ids.len()
        || !["personal", "team"].contains(&scope.as_str())
    {
        return Err("Choose up to ten images and a valid upload scope.".into());
    }
    let _guard = state.0.lock().await;
    let ws = workspace.inner().clone();
    let local_images = super::projects::work(move || {
        item_ids
            .iter()
            .map(|id| {
                let item = ws.media_library_item_by_id(id)?;
                let (name, bytes) = ws.media_library_upload_bytes(id)?;
                Ok((id.clone(), name, bytes, item.hash))
            })
            .collect::<Result<Vec<_>, apex_core::Error>>()
    })
    .await
    .map_err(|error| error.message)?;
    let mut local_refs = Vec::with_capacity(local_images.len());
    let mut upload_items = Vec::with_capacity(local_images.len());
    for (id, name, bytes, hash) in local_images {
        upload_items.push(apex_application::team_sync::HostedImageUpload {
            client_id: id.clone(),
            name: name.clone(),
            bytes,
        });
        local_refs.push((id, name, hash));
    }
    let uploaded = apex_application::team_sync::upload_hosted_images(
        &server_url,
        &team_id,
        &scope,
        upload_items,
    )
    .await
    .map_err(|error| error.to_string())?;

    let ws = workspace.inner().clone();
    let uploaded_images = uploaded.images.clone();
    let selected_team_id = team_id.clone();
    super::projects::work(move || {
        for image in uploaded_images {
            let correlated = image
                .client_id
                .as_deref()
                .filter(|client_id| local_refs.iter().any(|(id, _, _)| id == client_id));
            for (id, name, hash) in &local_refs {
                let matches = correlated.map_or_else(
                    || {
                        (hash.as_deref() == Some(image.hash.as_str()) || name == &image.name)
                            && local_refs
                                .iter()
                                .filter(|(_, candidate_name, candidate_hash)| {
                                    candidate_name == name
                                        || candidate_hash.as_deref() == Some(image.hash.as_str())
                                })
                                .count()
                                == 1
                    },
                    |client_id| id == client_id,
                );
                if !matches {
                    continue;
                }
                let team = (image.owner_scope == "team").then_some(selected_team_id.as_str());
                ws.set_media_library_remote(
                    id,
                    &image.id,
                    &image.image_url,
                    &image.owner_scope,
                    team,
                )?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|error| error.message)?;
    Ok(uploaded)
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
