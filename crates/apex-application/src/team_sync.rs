//! Shared account and team administration client for desktop, CLI, and MCP adapters.
//! Session tokens are stored only in the operating system credential vault.

use apex_core::Error;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION},
    Client, Response, Url,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use std::{net::IpAddr, time::Duration};
use zeroize::{Zeroize, Zeroizing};

pub mod project;

#[cfg(all(windows, not(test)))]
const CREDENTIAL_SERVICE: &str = "studio.apexdesigns.apexstudio";
#[cfg(all(windows, not(test)))]
const CREDENTIAL_ACCOUNT: &str = "team-sync-session";
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub product: String,
    pub api_version: String,
    pub server_version: String,
    pub server_instance_id: String,
    #[serde(default)]
    pub origin: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    pub name: String,
    pub role: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub teams: Vec<Team>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMember {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub server: ServerInfo,
    pub user: Option<User>,
    pub session_expired: bool,
    pub saved_session_for_different_server: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invitation {
    pub invitation_token: String,
    pub role: String,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAvailability {
    pub project_sync: bool,
    pub image_hosting: bool,
    pub invite_tracking: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedImage {
    pub id: String,
    pub name: String,
    pub image_url: String,
    pub hash: String,
    pub alt_text: String,
    pub default_link_url: Option<String>,
    pub owner_scope: String,
    pub created_at: String,
    #[serde(default)]
    pub reused: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedImages {
    pub images: Vec<HostedImage>,
    #[serde(default)]
    pub errors: Vec<serde_json::Value>,
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

#[derive(Deserialize, Serialize, Clone)]
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

fn invalid(message: &str) -> Error {
    Error::InvalidContent(message.to_owned())
}

fn parse_origin(server_url: &str) -> Result<Url, Error> {
    let url = Url::parse(server_url.trim())
        .map_err(|_| invalid("Enter a valid HTTPS server address."))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.host_str().is_none()
    {
        return Err(invalid(
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
        return Err(invalid(
            "Use HTTPS for team servers. HTTP is allowed only on this computer.",
        ));
    }
    Ok(url)
}

fn origin_text(url: &Url) -> String {
    url.origin().ascii_serialization()
}

fn client() -> Result<Client, Error> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| Error::Storage("Could not start a secure server connection.".into()))
}

fn endpoint(base: &Url, path: &str) -> Result<Url, Error> {
    base.join(path)
        .map_err(|_| invalid("The server address is invalid."))
}

async fn response_json<T: DeserializeOwned>(response: Response) -> Result<T, Error> {
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|_| Error::Storage("The server response was interrupted. Try again.".into()))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(invalid(
            "The server returned an unexpectedly large response.",
        ));
    }
    if !status.is_success() {
        let code = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|body| body.get("error")?.as_str().map(str::to_owned))
            .unwrap_or_default();
        return Err(invalid(match code.as_str() {
            "invalid_credentials" => "Email or password is incorrect.",
            "invalid_setup_token" => "The one-time setup key was not accepted.",
            "server_already_initialized" => "This server already has an Owner account.",
            "setup_unavailable" => "First-time setup is not enabled on this server.",
            "invitation_unavailable" => "This invitation is expired or has already been used.",
            "already_exists" => "An account with this email already exists.",
            "upload_capacity_reached" => {
                "The team server has reached its temporary upload limit. Retry synchronization after other uploads finish."
            }
            "blob_storage_capacity_reached" => {
                "The team server has reached its blob storage limit. Contact its administrator."
            }
            "authentication_required" | "invalid_session" => {
                "Your server session has expired. Sign in again."
            }
            "insufficient_team_role" => "Your account does not have permission for this action.",
            _ => {
                return Err(invalid(&format!(
                    "The server returned HTTP {}.",
                    status.as_u16()
                )))
            }
        }));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| invalid("The server returned data Apex Studio could not read."))
}

async fn server_info(base: &Url) -> Result<ServerInfo, Error> {
    let response = client()?
        .get(endpoint(base, "api/v1/server-info")?)
        .send()
        .await
        .map_err(|_| {
            Error::Storage("Could not reach this server. Check the address and connection.".into())
        })?;
    let mut info: ServerInfo = response_json(response).await?;
    if info.product != "Apex Sync Server" || info.api_version != "v1" {
        return Err(invalid(
            "This server does not support the Apex Studio sync API v1.",
        ));
    }
    info.origin = origin_text(base);
    Ok(info)
}

#[cfg(test)]
thread_local! {
    static TEST_SESSION: std::cell::RefCell<Option<SavedSession>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn load_session() -> Result<Option<SavedSession>, Error> {
    Ok(TEST_SESSION.with(|session| session.borrow().clone()))
}

#[cfg(all(windows, not(test)))]
fn vault() -> Result<keyring::Entry, Error> {
    keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT)
        .map_err(|_| Error::Storage("Windows Credential Manager is unavailable.".into()))
}

#[cfg(all(not(windows), not(test)))]
fn vault() -> Result<(), Error> {
    Err(invalid(
        "Secure team-server sessions are currently available on Windows.",
    ))
}

#[cfg(all(windows, not(test)))]
fn load_session() -> Result<Option<SavedSession>, Error> {
    match vault()?.get_secret() {
        Ok(bytes) => {
            let bytes = Zeroizing::new(bytes);
            serde_json::from_slice(&bytes).map(Some).map_err(|_| {
                invalid("The saved team-server session cannot be read. Sign in again.")
            })
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(Error::Storage(
            "Windows could not unlock the saved team-server session.".into(),
        )),
    }
}

#[cfg(all(not(windows), not(test)))]
fn load_session() -> Result<Option<SavedSession>, Error> {
    vault()?;
    Ok(None)
}

#[cfg(test)]
fn save_session(session: SavedSession) -> Result<(), Error> {
    TEST_SESSION.with(|saved| *saved.borrow_mut() = Some(session));
    Ok(())
}

#[cfg(all(windows, not(test)))]
fn save_session(session: SavedSession) -> Result<(), Error> {
    let bytes = Zeroizing::new(serde_json::to_vec(&session).map_err(|_| {
        Error::Storage("The team-server session could not be encoded securely.".into())
    })?);
    vault()?.set_secret(&bytes).map_err(|_| {
        Error::Storage("Windows could not securely save the team-server session.".into())
    })
}

#[cfg(all(not(windows), not(test)))]
fn save_session(_session: SavedSession) -> Result<(), Error> {
    vault()?;
    Ok(())
}

#[cfg(test)]
fn clear_session() -> Result<(), Error> {
    TEST_SESSION.with(|saved| *saved.borrow_mut() = None);
    Ok(())
}

#[cfg(all(windows, not(test)))]
fn clear_session() -> Result<(), Error> {
    match vault()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(Error::Storage(
            "Windows could not remove the saved team-server session.".into(),
        )),
    }
}

#[cfg(all(not(windows), not(test)))]
fn clear_session() -> Result<(), Error> {
    vault()?;
    Ok(())
}

fn auth_headers(token: &str) -> Result<HeaderMap, Error> {
    let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| invalid("The saved team-server session is invalid. Sign in again."))?;
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, value);
    Ok(headers)
}

async fn current_user(base: &Url, token: &str) -> Result<Response, Error> {
    client()?
        .get(endpoint(base, "api/v1/session")?)
        .headers(auth_headers(token)?)
        .send()
        .await
        .map_err(|_| {
            Error::Storage("Could not reach the team server. Check your connection.".into())
        })
}

async fn authenticate(
    base: Url,
    server: ServerInfo,
    path: &str,
    payload: serde_json::Value,
) -> Result<ConnectionStatus, Error> {
    let response = client()?
        .post(endpoint(&base, path)?)
        .json(&payload)
        .send()
        .await
        .map_err(|_| {
            Error::Storage("Could not reach the team server. Check your connection.".into())
        })?;
    let mut session: SessionResponse = response_json(response).await?;
    let user: User = response_json(current_user(&base, &session.access_token).await?).await?;
    let access_token = std::mem::take(&mut session.access_token);
    save_session(SavedSession {
        server_origin: server.origin.clone(),
        server_instance_id: server.server_instance_id.clone(),
        access_token,
    })?;
    Ok(ConnectionStatus {
        server,
        user: Some(user),
        session_expired: false,
        saved_session_for_different_server: false,
    })
}

async fn authenticated_session(server: &ServerInfo) -> Result<SavedSession, Error> {
    let saved = load_session()?.ok_or_else(|| invalid("Sign in to the team server first."))?;
    if saved.server_origin != server.origin || saved.server_instance_id != server.server_instance_id
    {
        return Err(invalid(
            "The saved session belongs to another server. Sign in again.",
        ));
    }
    Ok(saved)
}

async fn team_service_endpoint(
    server_url: &str,
    team_id: &str,
    suffix: &str,
) -> Result<(Client, Url, SavedSession), Error> {
    let team_id = uuid::Uuid::parse_str(team_id)
        .map_err(|_| invalid("Select a valid team before using its server services."))?;
    let base = parse_origin(server_url)?;
    let server = server_info(&base).await?;
    let session = authenticated_session(&server).await?;
    let path = format!("api/v1/teams/{team_id}/{suffix}");
    Ok((client()?, endpoint(&base, &path)?, session))
}

pub async fn service_availability(
    server_url: &str,
    team_id: &str,
) -> Result<ServiceAvailability, Error> {
    let (client, url, session) = team_service_endpoint(server_url, team_id, "services").await?;
    let response = client
        .get(url)
        .headers(auth_headers(&session.access_token)?)
        .send()
        .await
        .map_err(|_| Error::Storage("Could not reach the team server.".into()))?;
    response_json(response).await
}

pub async fn list_hosted_images(server_url: &str, team_id: &str) -> Result<HostedImages, Error> {
    let (client, mut url, session) =
        team_service_endpoint(server_url, team_id, "image-library").await?;
    url.query_pairs_mut().append_pair("scope", "available");
    let response = client
        .get(url)
        .headers(auth_headers(&session.access_token)?)
        .send()
        .await
        .map_err(|_| Error::Storage("Could not reach the team image library.".into()))?;
    response_json(response).await
}

pub async fn upload_hosted_images(
    server_url: &str,
    team_id: &str,
    scope: &str,
    images: Vec<(String, Vec<u8>)>,
) -> Result<HostedImages, Error> {
    if !["personal", "team"].contains(&scope) || images.is_empty() || images.len() > 10 {
        return Err(invalid(
            "Choose a personal or team scope and up to ten images.",
        ));
    }
    let (client, url, session) =
        team_service_endpoint(server_url, team_id, "image-library").await?;
    let mut form = reqwest::multipart::Form::new();
    for (name, bytes) in images {
        if bytes.is_empty() || bytes.len() > 40 * 1024 * 1024 {
            return Err(invalid(
                "Each hosted image must be between 1 byte and 40 MB.",
            ));
        }
        let safe_name = name
            .chars()
            .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
            .take(200)
            .collect::<String>();
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(if safe_name.is_empty() {
                "image.png".to_owned()
            } else {
                safe_name
            })
            .mime_str("image/png")
            .map_err(|_| invalid("Could not prepare the image upload."))?;
        form = form.part("images", part);
    }
    let mut url = url;
    url.query_pairs_mut().append_pair("scope", scope);
    let response = client
        .post(url)
        .headers(auth_headers(&session.access_token)?)
        .multipart(form)
        .send()
        .await
        .map_err(|_| Error::Storage("Could not reach the team image library.".into()))?;
    response_json(response).await
}

pub async fn set_hosted_image_scope(
    server_url: &str,
    team_id: &str,
    image_id: &str,
    scope: &str,
) -> Result<(), Error> {
    let image_id = uuid::Uuid::parse_str(image_id)
        .map_err(|_| invalid("Select a valid image to change its sharing."))?;
    if !["personal", "team"].contains(&scope) {
        return Err(invalid("Select personal or team sharing."));
    }
    let team_id = uuid::Uuid::parse_str(team_id)
        .map_err(|_| invalid("Select a valid team before using its server services."))?;
    let base = parse_origin(server_url)?;
    let server = server_info(&base).await?;
    let session = authenticated_session(&server).await?;
    let client = client()?;
    let url = endpoint(
        &base,
        &format!("api/v1/teams/{team_id}/image-library/{image_id}/sharing"),
    )?;
    let response = client
        .patch(url)
        .headers(auth_headers(&session.access_token)?)
        .json(&json!({"scope": scope}))
        .send()
        .await
        .map_err(|_| Error::Storage("Could not update image sharing.".into()))?;
    let _: serde_json::Value = response_json(response).await?;
    Ok(())
}

/// Validate a server address and return its API identity.
pub async fn check_server(server_url: &str) -> Result<ServerInfo, Error> {
    server_info(&parse_origin(server_url)?).await
}

/// Read the current account and team list without exposing the saved token.
pub async fn connection_status(server_url: &str) -> Result<ConnectionStatus, Error> {
    let base = parse_origin(server_url)?;
    let server = server_info(&base).await?;
    let Some(saved) = load_session()? else {
        return Ok(ConnectionStatus {
            server,
            user: None,
            session_expired: false,
            saved_session_for_different_server: false,
        });
    };
    if saved.server_origin != server.origin || saved.server_instance_id != server.server_instance_id
    {
        return Ok(ConnectionStatus {
            server,
            user: None,
            session_expired: false,
            saved_session_for_different_server: true,
        });
    }
    let response = current_user(&base, &saved.access_token).await?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        clear_session()?;
        return Ok(ConnectionStatus {
            server,
            user: None,
            session_expired: true,
            saved_session_for_different_server: false,
        });
    }
    let user: User = response_json(response).await?;
    Ok(ConnectionStatus {
        server,
        user: Some(user),
        session_expired: false,
        saved_session_for_different_server: false,
    })
}

/// Authenticate without accepting a password on the command line or writing it to disk.
pub async fn sign_in(
    server_url: &str,
    email: &str,
    password: &str,
) -> Result<ConnectionStatus, Error> {
    let password = Zeroizing::new(password.to_owned());
    let base = parse_origin(server_url)?;
    let server = server_info(&base).await?;
    authenticate(
        base,
        server,
        "api/v1/sessions",
        json!({ "email": email.trim(), "password": password.as_str() }),
    )
    .await
}

/// Accept an invitation and store the returned session in the OS credential vault.
pub async fn accept_invitation(
    server_url: &str,
    invitation_token: &str,
    email: &str,
    display_name: &str,
    password: &str,
) -> Result<ConnectionStatus, Error> {
    let password = Zeroizing::new(password.to_owned());
    let base = parse_origin(server_url)?;
    let server = server_info(&base).await?;
    authenticate(
        base,
        server,
        "api/v1/invitations/accept",
        json!({
            "invitationToken": invitation_token.trim(),
            "email": email.trim(),
            "displayName": display_name.trim(),
            "password": password.as_str(),
        }),
    )
    .await
}

/// Initialize a new sync server with its one-time bootstrap credential.
pub async fn initial_setup(
    server_url: &str,
    bootstrap_token: &str,
    team_name: &str,
    email: &str,
    display_name: &str,
    password: &str,
) -> Result<ConnectionStatus, Error> {
    let bootstrap_token = Zeroizing::new(bootstrap_token.to_owned());
    let password = Zeroizing::new(password.to_owned());
    let base = parse_origin(server_url)?;
    let server = server_info(&base).await?;
    authenticate(
        base,
        server,
        "api/v1/setup",
        json!({
            "bootstrapToken": bootstrap_token.trim(),
            "teamName": team_name.trim(),
            "email": email.trim(),
            "displayName": display_name.trim(),
            "password": password.as_str(),
        }),
    )
    .await
}

/// Create an invitation for an allowed member role.
pub async fn create_invitation(
    server_url: &str,
    team_id: &str,
    role: &str,
) -> Result<Invitation, Error> {
    let team_id = uuid::Uuid::parse_str(team_id)
        .map_err(|_| invalid("Select a valid team before creating an invitation."))?;
    if !matches!(role, "admin" | "designer" | "read_only") {
        return Err(invalid("Choose a valid invitation role."));
    }
    let base = parse_origin(server_url)?;
    let server = server_info(&base).await?;
    let saved = authenticated_session(&server).await?;
    let response = client()?
        .post(endpoint(
            &base,
            &format!("api/v1/teams/{team_id}/invitations"),
        )?)
        .headers(auth_headers(&saved.access_token)?)
        .json(&json!({ "role": role }))
        .send()
        .await
        .map_err(|_| {
            Error::Storage("Could not reach the team server. Check your connection.".into())
        })?;
    let invitation: Invitation = response_json(response).await?;
    if invitation.expires_in_seconds == 0 || invitation.invitation_token.is_empty() {
        return Err(invalid(
            "The team server returned an invalid invitation code.",
        ));
    }
    Ok(invitation)
}

/// List members for a team the current account belongs to.
pub async fn list_members(server_url: &str, team_id: &str) -> Result<Vec<TeamMember>, Error> {
    let team_id = uuid::Uuid::parse_str(team_id)
        .map_err(|_| invalid("Select a valid team before viewing its members."))?;
    let base = parse_origin(server_url)?;
    let server = server_info(&base).await?;
    let saved = authenticated_session(&server).await?;
    let response = client()?
        .get(endpoint(&base, &format!("api/v1/teams/{team_id}/members"))?)
        .headers(auth_headers(&saved.access_token)?)
        .send()
        .await
        .map_err(|_| {
            Error::Storage("Could not reach the team server. Check your connection.".into())
        })?;
    response_json(response).await
}

/// Remove a member. The server enforces owner/admin role hierarchy.
pub async fn remove_member(server_url: &str, team_id: &str, user_id: &str) -> Result<(), Error> {
    let team_id = uuid::Uuid::parse_str(team_id)
        .map_err(|_| invalid("Select a valid team before removing a member."))?;
    let user_id = uuid::Uuid::parse_str(user_id)
        .map_err(|_| invalid("Select a valid team member to remove."))?;
    let base = parse_origin(server_url)?;
    let server = server_info(&base).await?;
    let saved = authenticated_session(&server).await?;
    let response = client()?
        .delete(endpoint(
            &base,
            &format!("api/v1/teams/{team_id}/members/{user_id}"),
        )?)
        .headers(auth_headers(&saved.access_token)?)
        .send()
        .await
        .map_err(|_| {
            Error::Storage("Could not reach the team server. Check your connection.".into())
        })?;
    if response.status() == reqwest::StatusCode::NO_CONTENT {
        Ok(())
    } else {
        let _: serde_json::Value = response_json(response).await?;
        Err(invalid("The server returned an unexpected response."))
    }
}

/// Remove the stored session and best-effort revoke it on its original server.
pub async fn disconnect() -> Result<(), Error> {
    if let Some(saved) = load_session()? {
        if let Ok(base) = parse_origin(&saved.server_origin) {
            if let Ok(server) = server_info(&base).await {
                if server.server_instance_id == saved.server_instance_id {
                    if let (Ok(headers), Ok(url), Ok(client)) = (
                        auth_headers(&saved.access_token),
                        endpoint(&base, "api/v1/sessions/current"),
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    struct MockResponse {
        method: &'static str,
        path: String,
        status: u16,
        body: String,
        authenticated: bool,
        expected_json: Option<serde_json::Value>,
    }

    fn server_info_response() -> MockResponse {
        MockResponse {
            method: "GET",
            path: "/api/v1/server-info".into(),
            status: 200,
            body: r#"{"product":"Apex Sync Server","apiVersion":"v1","serverVersion":"test","serverInstanceId":"mock-instance"}"#.into(),
            authenticated: false,
            expected_json: None,
        }
    }

    fn current_user_response() -> MockResponse {
        MockResponse {
            method: "GET",
            path: "/api/v1/session".into(),
            status: 200,
            body: r#"{"id":"user-1","email":"designer@example.test","displayName":"Designer","teams":[{"id":"00000000-0000-0000-0000-000000000001","name":"Apex","role":"owner"}]}"#.into(),
            authenticated: true,
            expected_json: None,
        }
    }

    async fn mock_server(responses: Vec<MockResponse>) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock server should bind");
        let address = listener.local_addr().expect("mock address should exist");
        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            for expected in responses {
                let (mut stream, _) = listener.accept().await.expect("request should connect");
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    stream
                        .read_exact(&mut byte)
                        .await
                        .expect("request headers should be readable");
                    request.push(byte[0]);
                }
                let request_text = String::from_utf8_lossy(&request);
                let request_line = request_text.lines().next().unwrap_or_default();
                let headers = request_text.to_lowercase();
                assert!(
                    request_line.starts_with(&format!("{} {} ", expected.method, expected.path))
                );
                assert_eq!(
                    headers.contains("authorization: bearer mock-session"),
                    expected.authenticated,
                    "unexpected authorization header for {request_line}"
                );
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; length];
                if length > 0 {
                    stream
                        .read_exact(&mut body)
                        .await
                        .expect("request body should be readable");
                }
                if let Some(expected_json) = expected.expected_json {
                    let actual: serde_json::Value =
                        serde_json::from_slice(&body).expect("request body should be JSON");
                    assert_eq!(actual, expected_json);
                }
                let reason = if expected.status == 204 {
                    "No Content"
                } else {
                    "OK"
                };
                let response = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    expected.status,
                    reason,
                    expected.body.len(),
                    expected.body
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("mock response should be written");
            }
        });
        (format!("http://{address}"), server)
    }

    #[test]
    fn permits_https_and_loopback_http_only() {
        assert!(parse_origin("https://sync.example.test").is_ok());
        assert!(parse_origin("http://127.0.0.1:8080").is_ok());
        assert!(parse_origin("http://localhost:8080").is_ok());
        assert!(parse_origin("http://sync.example.test").is_err());
        assert!(parse_origin("https://user:secret@sync.example.test").is_err());
        assert!(parse_origin("https://sync.example.test/api").is_err());
    }

    #[test]
    fn invitation_roles_are_explicitly_limited() {
        for role in ["admin", "designer", "read_only"] {
            assert!(matches!(role, "admin" | "designer" | "read_only"));
        }
        assert!(!matches!("owner", "admin" | "designer" | "read_only"));
    }

    #[test]
    fn sign_in_saves_a_server_scoped_session_and_status_reads_the_account() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should initialize");
        runtime.block_on(async {
            let (origin, server) = mock_server(vec![
                server_info_response(),
                MockResponse {
                    method: "POST",
                    path: "/api/v1/sessions".into(),
                    status: 200,
                    body: r#"{"accessToken":"mock-session","tokenType":"Bearer","expiresInSeconds":3600,"userId":"00000000-0000-0000-0000-000000000003"}"#.into(),
                    authenticated: false,
                    expected_json: Some(json!({"email":"designer@example.test","password":"correct horse"})),
                },
                MockResponse {
                    method: "GET",
                    path: "/api/v1/session".into(),
                    status: 200,
                    body: r#"{"id":"user-1","email":"designer@example.test","displayName":"Designer","teams":[{"id":"00000000-0000-0000-0000-000000000001","name":"Apex","role":"owner"}]}"#.into(),
                    authenticated: true,
                    expected_json: None,
                },
                server_info_response(),
                MockResponse {
                    method: "GET",
                    path: "/api/v1/session".into(),
                    status: 200,
                    body: r#"{"id":"user-1","email":"designer@example.test","displayName":"Designer","teams":[{"id":"00000000-0000-0000-0000-000000000001","name":"Apex","role":"owner"}]}"#.into(),
                    authenticated: true,
                    expected_json: None,
                },
            ])
            .await;
            let signed_in = sign_in(&origin, " designer@example.test ", "correct horse")
                .await
                .expect("sign in should succeed");
            assert_eq!(signed_in.user.as_ref().unwrap().display_name, "Designer");
            let status = connection_status(&origin)
                .await
                .expect("session status should succeed");
            assert_eq!(status.user.as_ref().unwrap().teams[0].role, "owner");
            server.await.expect("mock server should finish");
            clear_session().expect("test session should clear");
        });
    }

    #[test]
    fn owner_setup_and_invitation_acceptance_use_the_expected_api_payloads() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should initialize");
        runtime.block_on(async {
            let (origin, server) = mock_server(vec![
                server_info_response(),
                MockResponse {
                    method: "POST",
                    path: "/api/v1/setup".into(),
                    status: 200,
                    body: r#"{"accessToken":"mock-session","tokenType":"Bearer","expiresInSeconds":3600,"userId":"00000000-0000-0000-0000-000000000003"}"#.into(),
                    authenticated: false,
                    expected_json: Some(json!({
                        "bootstrapToken":"bootstrap-once",
                        "teamName":"Apex Designs",
                        "email":"owner@example.test",
                        "displayName":"Owner",
                        "password":"correct horse"
                    })),
                },
                current_user_response(),
                server_info_response(),
                MockResponse {
                    method: "POST",
                    path: "/api/v1/invitations/accept".into(),
                    status: 200,
                    body: r#"{"accessToken":"mock-session","tokenType":"Bearer","expiresInSeconds":3600,"userId":"00000000-0000-0000-0000-000000000003"}"#.into(),
                    authenticated: false,
                    expected_json: Some(json!({
                        "invitationToken":"invite-once",
                        "email":"designer@example.test",
                        "displayName":"Designer",
                        "password":"correct horse"
                    })),
                },
                current_user_response(),
            ])
            .await;
            initial_setup(
                &origin,
                " bootstrap-once ",
                " Apex Designs ",
                " owner@example.test ",
                " Owner ",
                "correct horse",
            )
            .await
            .expect("owner setup should succeed");
            accept_invitation(
                &origin,
                " invite-once ",
                " designer@example.test ",
                " Designer ",
                "correct horse",
            )
            .await
            .expect("invitation acceptance should succeed");
            server.await.expect("mock server should finish");
            clear_session().expect("test session should clear");
        });
    }

    #[test]
    fn expired_invitation_is_reported_without_creating_a_session() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should initialize");
        runtime.block_on(async {
            let (origin, server) = mock_server(vec![
                server_info_response(),
                MockResponse {
                    method: "POST",
                    path: "/api/v1/invitations/accept".into(),
                    status: 410,
                    body: r#"{"error":"invitation_unavailable"}"#.into(),
                    authenticated: false,
                    expected_json: None,
                },
            ])
            .await;
            let error = accept_invitation(
                &origin,
                "expired-code",
                "designer@example.test",
                "Designer",
                "correct horse",
            )
            .await
            .expect_err("expired invitation should be rejected");
            assert!(error
                .to_string()
                .contains("expired or has already been used"));
            server.await.expect("mock server should finish");
            assert!(load_session().unwrap().is_none());
        });
    }

    #[test]
    fn member_permission_failure_is_reported_without_removing_anyone() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should initialize");
        runtime.block_on(async {
            let team = "00000000-0000-0000-0000-000000000001";
            let member = "00000000-0000-0000-0000-000000000002";
            let (origin, server) = mock_server(vec![
                server_info_response(),
                MockResponse {
                    method: "DELETE",
                    path: format!("/api/v1/teams/{team}/members/{member}"),
                    status: 403,
                    body: r#"{"error":"insufficient_team_role"}"#.into(),
                    authenticated: true,
                    expected_json: None,
                },
            ])
            .await;
            save_session(SavedSession {
                server_origin: origin.clone(),
                server_instance_id: "mock-instance".into(),
                access_token: "mock-session".into(),
            })
            .expect("test session should be set");
            let error = remove_member(&origin, team, member)
                .await
                .expect_err("server should reject a lower-rank removal");
            assert!(error.to_string().contains("does not have permission"));
            server.await.expect("mock server should finish");
            clear_session().expect("test session should clear");
        });
    }

    #[test]
    fn invitations_members_and_removal_use_authenticated_team_routes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should initialize");
        runtime.block_on(async {
            let team = "00000000-0000-0000-0000-000000000001";
            let member = "00000000-0000-0000-0000-000000000002";
            let (origin, server) = mock_server(vec![
                server_info_response(),
                MockResponse {
                    method: "POST",
                    path: format!("/api/v1/teams/{team}/invitations"),
                    status: 200,
                    body: r#"{"invitationToken":"single-use-code","role":"designer","expiresInSeconds":3600}"#.into(),
                    authenticated: true,
                    expected_json: Some(json!({"role":"designer"})),
                },
                server_info_response(),
                MockResponse {
                    method: "GET",
                    path: format!("/api/v1/teams/{team}/members"),
                    status: 200,
                    body: format!(r#"[{{"id":"{member}","email":"member@example.test","displayName":"Member","role":"designer"}}]"#),
                    authenticated: true,
                    expected_json: None,
                },
                server_info_response(),
                MockResponse {
                    method: "DELETE",
                    path: format!("/api/v1/teams/{team}/members/{member}"),
                    status: 204,
                    body: String::new(),
                    authenticated: true,
                    expected_json: None,
                },
            ])
            .await;
            save_session(SavedSession {
                server_origin: origin.clone(),
                server_instance_id: "mock-instance".into(),
                access_token: "mock-session".into(),
            })
            .expect("test session should be set");
            let invite = create_invitation(&origin, team, "designer")
                .await
                .expect("invitation should be created");
            assert_eq!(invite.invitation_token, "single-use-code");
            let members = list_members(&origin, team)
                .await
                .expect("member list should load");
            assert_eq!(members.len(), 1);
            assert_eq!(members[0].id, member);
            remove_member(&origin, team, member)
                .await
                .expect("member should be removed");
            server.await.expect("mock server should finish");
            clear_session().expect("test session should clear");
        });
    }

    #[test]
    fn invitation_rejects_owner_role_before_contacting_server() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should initialize");
        let result = runtime.block_on(create_invitation(
            "http://127.0.0.1:1",
            "00000000-0000-0000-0000-000000000001",
            "owner",
        ));
        assert!(result.is_err());
    }

    #[test]
    fn unreachable_team_server_returns_a_connection_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("offline test port should be allocated");
        let address = listener
            .local_addr()
            .expect("offline test port should exist");
        drop(listener);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should initialize");
        let error = runtime
            .block_on(check_server(&format!("http://{address}")))
            .expect_err("closed local port should be unavailable");
        assert!(error.to_string().contains("Could not reach"));
    }

    #[test]
    fn server_check_reads_api_identity_from_a_loopback_mock() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should initialize");
        runtime.block_on(async {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("mock server should bind");
            let address = listener.local_addr().expect("mock address should exist");
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.expect("request should connect");
                let mut request = Vec::new();
                let mut byte = [0; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    stream
                        .read_exact(&mut byte)
                        .await
                        .expect("request headers should be readable");
                    request.push(byte[0]);
                }
                assert!(String::from_utf8_lossy(&request).starts_with("GET /api/v1/server-info "));
                let body = r#"{"product":"Apex Sync Server","apiVersion":"v1","serverVersion":"test","serverInstanceId":"mock-instance"}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("mock response should be written");
            });
            let info = check_server(&format!("http://{address}")).await.unwrap();
            server.await.expect("mock server should finish");
            assert_eq!(info.product, "Apex Sync Server");
            assert_eq!(info.api_version, "v1");
            assert_eq!(info.origin, format!("http://{address}"));
        });
    }
}
