use std::{net::IpAddr, time::Duration};

use keyring::Entry;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION},
    Client, Response, Url,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use tauri::State;
use tokio::sync::Mutex;
use zeroize::{Zeroize, Zeroizing};
mod project_sync;
pub use project_sync::*;

const CREDENTIAL_SERVICE: &str = "studio.apexdesigns.apexstudio";
const CREDENTIAL_ACCOUNT: &str = "team-sync-session";
const MAX_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Default)]
pub struct TeamSyncState(pub Mutex<()>);

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncServerInfo {
    pub product: String,
    pub api_version: String,
    pub server_version: String,
    pub server_instance_id: String,
    #[serde(default)]
    pub origin: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTeam {
    pub id: String,
    pub name: String,
    pub role: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncUser {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub teams: Vec<SyncTeam>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTeamMember {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConnectionStatus {
    pub server: SyncServerInfo,
    pub user: Option<SyncUser>,
    pub session_expired: bool,
    pub saved_session_for_different_server: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncInvitation {
    pub invitation_token: String,
    pub role: String,
    pub expires_in_seconds: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    access_token: String,
}

impl Drop for SessionResponse {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedSession {
    server_origin: String,
    server_instance_id: String,
    access_token: String,
}

impl Drop for SavedSession {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}

fn error(message: &str) -> String {
    message.to_owned()
}

fn origin(server_url: &str) -> Result<Url, String> {
    let url =
        Url::parse(server_url.trim()).map_err(|_| error("Enter a valid HTTPS server address."))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.host_str().is_none()
    {
        return Err(error(
            "Enter the server address only, without a path or credentials.",
        ));
    }
    let local_http = url.scheme() == "http"
        && url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
    if url.scheme() != "https" && !local_http {
        return Err(error(
            "Use HTTPS for team servers. HTTP is allowed only on this computer.",
        ));
    }
    Ok(url)
}

fn origin_text(url: &Url) -> String {
    url.origin().ascii_serialization()
}

fn client() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| error("A secure server connection could not be started."))
}

fn api_url(base: &Url, path: &str) -> Result<Url, String> {
    base.join(path)
        .map_err(|_| error("The server address is invalid."))
}

async fn response_json<T: DeserializeOwned>(response: Response) -> Result<T, String> {
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|_| error("The server response was interrupted. Try again."))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(error("The server returned an unexpectedly large response."));
    }
    if !status.is_success() {
        let code = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|body| body.get("error")?.as_str().map(str::to_owned))
            .unwrap_or_default();
        return Err(match code.as_str() {
            "invalid_credentials" => error("Email or password is incorrect."),
            "invalid_setup_token" => error("The one-time setup key was not accepted."),
            "server_already_initialized" => error("This server already has an Owner account."),
            "setup_unavailable" => error("First-time setup is not enabled on this server."),
            "invitation_unavailable" => {
                error("This invitation is expired or has already been used.")
            }
            "already_exists" => error("An account with this email already exists."),
            "authentication_required" | "invalid_session" => {
                error("Your server session has expired. Sign in again.")
            }
            "insufficient_team_role" => {
                error("Your account does not have permission for this action.")
            }
            _ => error(&format!("The server returned HTTP {}.", status.as_u16())),
        });
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| error("The server returned data Apex Studio could not read."))
}

async fn server_info(base: &Url) -> Result<SyncServerInfo, String> {
    let response = client()?
        .get(api_url(base, "api/v1/server-info")?)
        .send()
        .await
        .map_err(|_| error("Could not reach this server. Check the address and connection."))?;
    let mut info: SyncServerInfo = response_json(response).await?;
    if info.product != "Apex Sync Server" || info.api_version != "v1" {
        return Err(error(
            "This server does not support the Apex Studio sync API v1.",
        ));
    }
    info.origin = origin_text(base);
    Ok(info)
}

fn vault() -> Result<Entry, String> {
    if !cfg!(target_os = "windows") {
        return Err(error(
            "Secure team-server sessions are currently available on Windows.",
        ));
    }
    Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT)
        .map_err(|_| error("Windows Credential Manager is unavailable."))
}

fn load_session() -> Result<Option<SavedSession>, String> {
    match vault()?.get_secret() {
        Ok(bytes) => {
            let bytes = Zeroizing::new(bytes);
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| error("The saved team-server session cannot be read. Sign in again."))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(error(
            "Windows could not unlock the saved team-server session.",
        )),
    }
}

fn save_session(session: SavedSession) -> Result<(), String> {
    let bytes = Zeroizing::new(
        serde_json::to_vec(&session)
            .map_err(|_| error("The team-server session could not be saved securely."))?,
    );
    vault()?
        .set_secret(&bytes)
        .map_err(|_| error("Windows could not securely save the team-server session."))
}

fn clear_session() -> Result<(), String> {
    match vault()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(error(
            "Windows could not remove the saved team-server session.",
        )),
    }
}

fn auth_headers(token: &str) -> Result<HeaderMap, String> {
    let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| error("The saved team-server session is invalid. Sign in again."))?;
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, value);
    Ok(headers)
}

async fn current_user(base: &Url, token: &str) -> Result<Response, String> {
    client()?
        .get(api_url(base, "api/v1/session")?)
        .headers(auth_headers(token)?)
        .send()
        .await
        .map_err(|_| error("Could not reach the team server. Check your connection."))
}

async fn authenticate(
    base: Url,
    server: SyncServerInfo,
    path: &str,
    payload: serde_json::Value,
) -> Result<SyncConnectionStatus, String> {
    let response = client()?
        .post(api_url(&base, path)?)
        .json(&payload)
        .send()
        .await
        .map_err(|_| error("Could not reach the team server. Check your connection."))?;
    let mut session: SessionResponse = response_json(response).await?;
    let user_response = current_user(&base, &session.access_token).await?;
    let user: SyncUser = response_json(user_response).await?;
    let access_token = std::mem::take(&mut session.access_token);
    save_session(SavedSession {
        server_origin: server.origin.clone(),
        server_instance_id: server.server_instance_id.clone(),
        access_token,
    })?;
    Ok(SyncConnectionStatus {
        server,
        user: Some(user),
        session_expired: false,
        saved_session_for_different_server: false,
    })
}

#[tauri::command]
pub async fn sync_check_server(
    server_url: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncServerInfo, String> {
    let _guard = state.0.lock().await;
    server_info(&origin(&server_url)?).await
}

#[tauri::command]
pub async fn sync_connection_status(
    server_url: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncConnectionStatus, String> {
    let _guard = state.0.lock().await;
    let base = origin(&server_url)?;
    let server = server_info(&base).await?;
    let Some(saved) = load_session()? else {
        return Ok(SyncConnectionStatus {
            server,
            user: None,
            session_expired: false,
            saved_session_for_different_server: false,
        });
    };
    if saved.server_origin != server.origin || saved.server_instance_id != server.server_instance_id
    {
        return Ok(SyncConnectionStatus {
            server,
            user: None,
            session_expired: false,
            saved_session_for_different_server: true,
        });
    }
    let response = current_user(&base, &saved.access_token).await?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        clear_session()?;
        return Ok(SyncConnectionStatus {
            server,
            user: None,
            session_expired: true,
            saved_session_for_different_server: false,
        });
    }
    let user = response_json(response).await?;
    Ok(SyncConnectionStatus {
        server,
        user: Some(user),
        session_expired: false,
        saved_session_for_different_server: false,
    })
}

#[tauri::command]
pub async fn sync_sign_in(
    server_url: String,
    email: String,
    password: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncConnectionStatus, String> {
    let _guard = state.0.lock().await;
    let password = Zeroizing::new(password);
    let base = origin(&server_url)?;
    let server = server_info(&base).await?;
    let payload = json!({ "email": email.trim(), "password": password.as_str() });
    authenticate(base, server, "api/v1/sessions", payload).await
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
    let password = Zeroizing::new(password);
    let base = origin(&server_url)?;
    let server = server_info(&base).await?;
    let payload = json!({
        "invitationToken": invitation_token.trim(),
        "email": email.trim(),
        "displayName": display_name.trim(),
        "password": password.as_str(),
    });
    authenticate(base, server, "api/v1/invitations/accept", payload).await
}

#[tauri::command]
pub async fn sync_create_invitation(
    server_url: String,
    team_id: String,
    role: String,
    state: State<'_, TeamSyncState>,
) -> Result<SyncInvitation, String> {
    let _guard = state.0.lock().await;
    let team_id = uuid::Uuid::parse_str(&team_id)
        .map_err(|_| error("Select a valid team before creating an invitation."))?;
    if !matches!(role.as_str(), "admin" | "designer" | "read_only") {
        return Err(error("Choose a valid invitation role."));
    }

    let base = origin(&server_url)?;
    let server = server_info(&base).await?;
    let Some(saved) = load_session()? else {
        return Err(error(
            "Sign in to the team server before inviting a member.",
        ));
    };
    if saved.server_origin != server.origin || saved.server_instance_id != server.server_instance_id
    {
        return Err(error(
            "Sign in to this team server before inviting a member.",
        ));
    }

    let response = client()?
        .post(api_url(
            &base,
            &format!("api/v1/teams/{team_id}/invitations"),
        )?)
        .headers(auth_headers(&saved.access_token)?)
        .json(&json!({ "role": role }))
        .send()
        .await
        .map_err(|_| error("Could not reach the team server. Check your connection."))?;
    let invitation: SyncInvitation = response_json(response).await?;
    if invitation.expires_in_seconds == 0 || invitation.invitation_token.is_empty() {
        return Err(error(
            "The team server returned an invalid invitation code.",
        ));
    }
    Ok(invitation)
}

#[tauri::command]
pub async fn sync_list_members(
    server_url: String,
    team_id: String,
    state: State<'_, TeamSyncState>,
) -> Result<Vec<SyncTeamMember>, String> {
    let _guard = state.0.lock().await;
    let team_id = uuid::Uuid::parse_str(&team_id)
        .map_err(|_| error("Select a valid team before viewing its members."))?;
    let base = origin(&server_url)?;
    let server = server_info(&base).await?;
    let Some(saved) = load_session()? else {
        return Err(error("Sign in to the team server to view its members."));
    };
    if saved.server_origin != server.origin || saved.server_instance_id != server.server_instance_id
    {
        return Err(error("Sign in to this team server before viewing members."));
    }

    let response = client()?
        .get(api_url(&base, &format!("api/v1/teams/{team_id}/members"))?)
        .headers(auth_headers(&saved.access_token)?)
        .send()
        .await
        .map_err(|_| error("Could not reach the team server. Check your connection."))?;
    response_json(response).await
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
    let bootstrap_token = Zeroizing::new(bootstrap_token);
    let password = Zeroizing::new(password);
    let base = origin(&server_url)?;
    let server = server_info(&base).await?;
    let payload = json!({
        "bootstrapToken": bootstrap_token.trim(),
        "teamName": team_name.trim(),
        "email": email.trim(),
        "displayName": display_name.trim(),
        "password": password.as_str(),
    });
    authenticate(base, server, "api/v1/setup", payload).await
}

#[tauri::command]
pub async fn sync_disconnect(state: State<'_, TeamSyncState>) -> Result<(), String> {
    let _guard = state.0.lock().await;
    if let Some(saved) = load_session()? {
        if let Ok(base) = origin(&saved.server_origin) {
            if let Ok(server) = server_info(&base).await {
                if server.server_instance_id == saved.server_instance_id {
                    if let (Ok(headers), Ok(url), Ok(client)) = (
                        auth_headers(&saved.access_token),
                        api_url(&base, "api/v1/sessions/current"),
                        client(),
                    ) {
                        let _ = client.delete(url).headers(headers).send().await;
                    }
                }
            }
        }
    }
    clear_session()
}
