use super::*;
use super::{ServerInfo as SyncServerInfo, Team as SyncTeam, User as SyncUser};
use apex_storage::{ProjectSnapshot, ProjectSync, SyncBlob, Workspace};
use futures_util::{stream, StreamExt, TryStreamExt};
use reqwest::{Client, Response, StatusCode, Url};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    io::{Read, Seek, SeekFrom, Write},
    sync::Arc,
};
use tokio::sync::Mutex;

fn error(message: &str) -> String {
    message.to_owned()
}

fn origin(server_url: &str) -> Result<Url, String> {
    super::parse_origin(server_url).map_err(|error| error.to_string())
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
        .map_err(|_| error("Could not start a secure server connection."))
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
        return Err(match status {
            StatusCode::UNAUTHORIZED => error("Your server session has expired. Sign in again."),
            StatusCode::FORBIDDEN => error("Your team role does not allow this operation."),
            StatusCode::CONFLICT => error("The project changed on another device."),
            _ => format!("The sync server returned HTTP {}.", status.as_u16()),
        });
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| error("The server returned data Apex Studio could not read."))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSyncChoice {
    pub enabled: bool,
    pub server_url: String,
    pub team_id: String,
}
pub struct SyncDestination {
    pub origin: String,
    pub instance: String,
    pub team: String,
}
struct Context {
    base: Url,
    session: SavedSession,
    server: SyncServerInfo,
    team: SyncTeam,
    can_apply: Arc<dyn Fn() -> bool + Send + Sync>,
}
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedDecalCode {
    pub content_hash: String,
    pub decal_asset_id: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SharedDecalCodeBatchRequest {
    codes: Vec<SharedDecalCode>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedDecalCodeBatchResponse {
    codes: Vec<SharedDecalCode>,
}
const SHARED_DECAL_CODE_ATTEMPTS: usize = 3;
const MAX_CONCURRENT_BLOB_UPLOADS: usize = 4;
const BLOB_UPLOAD_MEMORY_BUDGET_MIB: usize = 128;

async fn post_shared_decal_codes(
    http: &Client,
    url: Url,
    headers: HeaderMap,
    request: &SharedDecalCodeBatchRequest,
) -> Result<SharedDecalCodeBatchResponse, String> {
    for attempt in 0..SHARED_DECAL_CODE_ATTEMPTS {
        let response = match http
            .post(url.clone())
            .headers(headers.clone())
            .json(request)
            .send()
            .await
        {
            Ok(response) => response,
            Err(network_error)
                if attempt + 1 < SHARED_DECAL_CODE_ATTEMPTS
                    && (network_error.is_timeout() || network_error.is_connect()) =>
            {
                tokio::time::sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
                continue;
            }
            Err(network_error) => {
                let reason = if network_error.is_timeout() {
                    "timed out"
                } else if network_error.is_connect() {
                    "could not connect"
                } else {
                    "request failed"
                };
                return Err(error(&format!(
                    "Could not check team-shared Roblox decal codes: {reason} after {} attempt(s).",
                    attempt + 1
                )));
            }
        };

        if attempt + 1 < SHARED_DECAL_CODE_ATTEMPTS
            && matches!(
                response.status(),
                StatusCode::BAD_GATEWAY
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::GATEWAY_TIMEOUT
            )
        {
            tokio::time::sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
            continue;
        }

        return response_json(response).await;
    }

    Err(error(
        "Could not check team-shared Roblox decal codes after retrying.",
    ))
}

fn transfer_client() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15 * 60))
        .build()
        .map_err(|_| error("Could not start a project file transfer."))
}
fn ensure_enabled(ws: &Workspace, id: &str) -> Result<(), String> {
    if !ws
        .project_sync(id)
        .map_err(|e| e.to_string())?
        .is_some_and(|b| b.enabled)
    {
        return Err(error(
            "Synchronization was disabled. Your local copy has been kept.",
        ));
    }
    Ok(())
}
fn worker_lock() -> &'static Mutex<()> {
    static WORKER: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    WORKER.get_or_init(|| Mutex::new(()))
}
impl Context {
    fn url(&self, path: &str) -> Result<Url, String> {
        api_url(&self.base, path)
    }
    fn headers(&self) -> Result<HeaderMap, String> {
        auth_headers(&self.session.access_token).map_err(|error| error.to_string())
    }
    fn can_apply(&self) -> bool {
        (self.can_apply)()
    }
    fn project_path(&self, id: &str) -> String {
        format!("api/v1/teams/{}/projects/{id}", self.team.id)
    }
}

async fn sync_shared_decal_codes(
    c: &Context,
    ws: &Workspace,
    project_id: &str,
) -> Result<Vec<SharedDecalCode>, String> {
    let account_id = crate::load_roblox_credentials()
        .ok()
        .flatten()
        .map(|credentials| credentials.account_id.clone())
        .unwrap_or_default();
    let codes = ws
        .livery_decal_codes(project_id, &account_id)
        .map_err(|error| error.to_string())?;
    let mut local_codes = BTreeMap::<String, Option<String>>::new();
    for (hash, local_id) in codes {
        if let Some(existing) = local_codes.get_mut(&hash) {
            match (existing.as_deref(), local_id.as_deref()) {
                (Some(existing), Some(incoming)) if existing != incoming => {
                    return Err(error(
                        "This image has multiple Roblox decal IDs in the project. Resolve the local codes before syncing.",
                    ));
                }
                (None, Some(_)) => *existing = local_id,
                _ => {}
            }
        } else {
            local_codes.insert(hash, local_id);
        }
    }

    if local_codes.is_empty() {
        return Ok(Vec::new());
    }

    // Send the project’s code manifest in one small request instead of one request per texture.
    let http = client()?;
    let request = SharedDecalCodeBatchRequest {
        codes: local_codes
            .iter()
            .map(|(content_hash, decal_asset_id)| SharedDecalCode {
                content_hash: content_hash.clone(),
                decal_asset_id: if c.team.role == "read_only" {
                    None
                } else {
                    decal_asset_id.clone()
                },
            })
            .collect(),
    };
    let endpoint = format!("api/v1/teams/{}/decal-codes/batch", c.team.id);
    let url = c.url(&endpoint)?;
    let headers = c.headers()?;
    let mut shared_codes = Vec::new();
    for batch in request.codes.chunks(5_000) {
        let response = post_shared_decal_codes(
            &http,
            url.clone(),
            headers.clone(),
            &SharedDecalCodeBatchRequest {
                codes: batch.to_vec(),
            },
        )
        .await?;
        shared_codes.extend(response.codes);
    }
    let mut accepted_codes = Vec::with_capacity(shared_codes.len());
    for shared in shared_codes {
        let Some(local_id) = local_codes.get(&shared.content_hash) else {
            return Err(error(
                "The team server returned an unexpected Roblox decal code.",
            ));
        };
        let Some(shared_id) = shared.decal_asset_id else {
            continue;
        };
        if local_id.as_deref().is_some_and(|id| id != shared_id) {
            return Err(error("This image already has a different Roblox decal ID in the team catalog. Resolve the existing code before syncing."));
        }
        if !apex_core::valid_hash(&shared.content_hash) {
            return Err(error(
                "The team server returned an invalid image hash for a Roblox decal code.",
            ));
        }
        accepted_codes.push((shared.content_hash, shared_id));
    }
    ws.save_shared_decal_codes(project_id, &c.team.id, &accepted_codes)
        .map_err(|e| match e {
            apex_core::Error::Conflict => error("A different Roblox decal code is already shared for this image. Review the team code before continuing."),
            other => other.to_string(),
        })?;
    Ok(accepted_codes
        .into_iter()
        .map(|(content_hash, decal_asset_id)| SharedDecalCode {
            content_hash,
            decal_asset_id: Some(decal_asset_id),
        })
        .collect())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPreview {
    pub team: SyncTeam,
    pub remote_projects: Vec<RemoteProject>,
    pub linked_projects: Vec<ProjectSync>,
}

pub async fn remote_livery_thumbnail(
    ws: &Workspace,
    project_id: &str,
    hash: &str,
) -> Result<Vec<u8>, String> {
    if !apex_core::valid_hash(hash)
        || !ws
            .remote_livery_blob(project_id, hash)
            .map_err(|e| e.to_string())?
    {
        return Err(error(
            "This image is not an eligible team-shared livery asset.",
        ));
    }
    let binding = ws
        .project_sync(project_id)
        .map_err(|e| e.to_string())?
        .filter(|binding| binding.enabled)
        .ok_or_else(|| error("This project is not connected to its team server."))?;
    let context = context(&binding.origin, &binding.team_id).await?;
    let response = transfer_client()?
        .get(context.url(&format!(
            "api/v1/teams/{}/projects/{project_id}/blobs/{hash}",
            context.team.id
        ))?)
        .headers(context.headers()?)
        .send()
        .await
        .map_err(|_| error("Could not stream the team livery preview."))?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|size| size > 80 * 1024 * 1024)
    {
        return Err(error(
            "The team livery preview is unavailable or too large.",
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| error("The team livery preview stream was interrupted."))?;
    if bytes.len() > 80 * 1024 * 1024 || format!("{:x}", Sha256::digest(&bytes)) != hash {
        return Err(error(
            "The streamed team livery preview failed its integrity check.",
        ));
    }
    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| error("The team livery preview is not a supported image."))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| error("The team livery preview is not a supported image."))?;
    if width > 8000 || height > 8000 || u64::from(width) * u64::from(height) > 64_000_000 {
        return Err(error(
            "The streamed livery preview exceeds the image size limit.",
        ));
    }
    let decoded = image::load_from_memory(&bytes)
        .map_err(|_| error("The team livery preview is not a supported image."))?;
    let preview = decoded.thumbnail(480, 320);
    let mut output = std::io::Cursor::new(Vec::new());
    preview
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| error("Could not prepare the streamed livery preview."))?;
    Ok(output.into_inner())
}
async fn context(server_url: &str, team_id: &str) -> Result<Context, String> {
    let team_id = uuid::Uuid::parse_str(team_id)
        .map_err(|_| error("Choose a team in Settings → Team sync."))?
        .to_string();
    let base = origin(server_url)?;
    let server = server_info(&base)
        .await
        .map_err(|error| error.to_string())?;
    let session = load_session()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| error("Sign in under Settings → Team sync."))?;
    if session.server_origin != origin_text(&base)
        || session.server_instance_id != server.server_instance_id
    {
        return Err(error(
            "The saved session belongs to another server. Sign in again.",
        ));
    }
    let response = current_user(&base, &session.access_token)
        .await
        .map_err(|error| error.to_string())?;
    let user: SyncUser = response_json(response).await?;
    let team = user
        .teams
        .into_iter()
        .find(|t| t.id == team_id)
        .ok_or_else(|| error("The project is linked to a team that is not available to this account. Check Settings → Team sync, then use Reconnect to current team in the project details. The existing server copy will be kept."))?;
    Ok(Context {
        base,
        server,
        session,
        team,
        can_apply: Arc::new(|| true),
    })
}
pub async fn validate_sync_choice(
    choice: &ProjectSyncChoice,
) -> Result<Option<SyncDestination>, String> {
    if !choice.enabled {
        return Ok(None);
    }
    let c = context(&choice.server_url, &choice.team_id).await?;
    if c.team.role == "read_only" {
        return Err(error("Read-only members cannot share local projects."));
    }
    Ok(Some(SyncDestination {
        origin: origin_text(&c.base),
        instance: c.server.server_instance_id,
        team: c.team.id,
    }))
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RemoteProject {
    pub id: String,
    pub name: String,
}

pub async fn preview_sync(
    server_url: &str,
    team_id: &str,
    workspace: &Workspace,
) -> Result<SyncPreview, String> {
    let c = context(server_url, team_id).await?;
    let remote_projects: Vec<RemoteProject> = sync_json(
        client()?
            .get(c.url(&format!("api/v1/teams/{}/projects", c.team.id))?)
            .headers(c.headers()?)
            .send()
            .await
            .map_err(|_| error("The sync server could not be reached."))?,
    )
    .await?;
    let origin = origin_text(&c.base);
    let linked_projects = workspace
        .project_sync_bindings()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|binding| {
            binding.enabled
                && binding.origin == origin
                && binding.server_instance == c.server.server_instance_id
                && binding.team_id == c.team.id
        })
        .collect();
    Ok(SyncPreview {
        team: c.team,
        remote_projects,
        linked_projects,
    })
}

pub async fn refresh_shared_decal_codes(
    server_url: &str,
    team_id: &str,
    project_id: &str,
    workspace: &Workspace,
) -> Result<Vec<SharedDecalCode>, String> {
    let c = context(server_url, team_id).await?;
    sync_shared_decal_codes(&c, workspace, project_id).await
}
pub async fn project_sync_settings(
    project_id: &str,
    workspace: &Workspace,
) -> Result<Option<ProjectSync>, String> {
    workspace
        .project_sync(project_id)
        .map_err(|e| e.to_string())
}
pub async fn list_project_sync_settings(workspace: &Workspace) -> Result<Vec<ProjectSync>, String> {
    workspace.project_sync_bindings().map_err(|e| e.to_string())
}
pub async fn set_project_sync(
    project_id: &str,
    choice: ProjectSyncChoice,
    workspace: &Workspace,
) -> Result<Option<ProjectSync>, String> {
    let destination = validate_sync_choice(&choice).await?;
    workspace
        .configure_project_sync(
            project_id,
            destination
                .as_ref()
                .map(|d| (d.origin.as_str(), d.instance.as_str(), d.team.as_str())),
        )
        .map_err(|e| e.to_string())?;
    workspace
        .project_sync(project_id)
        .map_err(|e| e.to_string())
}

pub async fn reconnect_project_sync(
    project_id: &str,
    choice: ProjectSyncChoice,
    workspace: &Workspace,
) -> Result<Option<ProjectSync>, String> {
    reconnect_project_sync_guarded(project_id, choice, workspace, Arc::new(|| true)).await
}

pub async fn reconnect_project_sync_guarded(
    project_id: &str,
    choice: ProjectSyncChoice,
    workspace: &Workspace,
    can_apply: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<Option<ProjectSync>, String> {
    if !choice.enabled {
        return Err(error("Choose a team before reconnecting this project."));
    }
    let _worker = worker_lock().lock().await;
    let mut c = context(&choice.server_url, &choice.team_id).await?;
    c.can_apply = can_apply;
    if c.team.role == "read_only" {
        return Err(error("Read-only members cannot reconnect shared projects."));
    }
    if !c.can_apply() {
        return Err(error("Finish editing before reconnecting this project."));
    }
    let projects: Vec<RemoteProject> = sync_json(
        client()?
            .get(c.url(&format!("api/v1/teams/{}/projects", c.team.id))?)
            .headers(c.headers()?)
            .send()
            .await
            .map_err(|_| error("The sync server could not be reached."))?,
    )
    .await?;
    let server_has_project = projects.iter().any(|project| project.id == project_id);
    workspace
        .reconnect_project_sync(
            project_id,
            &origin_text(&c.base),
            &c.server.server_instance_id,
            &c.team.id,
            server_has_project,
        )
        .map_err(|e| e.to_string())?;
    workspace
        .project_sync(project_id)
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Change {
    cursor: i64,
    record_type: String,
    record_id: String,
    revision: i64,
    data: Option<Value>,
    deleted: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    changes: Vec<Change>,
    next_cursor: i64,
}
#[derive(Deserialize)]
struct Revision {
    revision: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEntry {
    pub project_id: String,
    pub project_name: String,
    pub status: &'static str,
    pub message: String,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub uploaded: usize,
    pub downloaded: usize,
    pub checked: usize,
    pub up_to_date: usize,
    pub shared_local: usize,
    pub remote_projects: usize,
    pub errors: Vec<String>,
    pub entries: Vec<SyncEntry>,
}

#[derive(Debug)]
enum SyncOutcome {
    Uploaded,
    Downloaded,
    UpToDate,
    Skipped(&'static str),
}

async fn sync_json<T: DeserializeOwned>(mut r: Response) -> Result<T, String> {
    if r.status() == StatusCode::NO_CONTENT {
        return serde_json::from_value(Value::Null).map_err(|_| {
            error("The server returned an empty response where a project record was required.")
        });
    }
    if !r.status().is_success() {
        return Err(match r.status() {
            StatusCode::CONFLICT => error("Conflict: this project changed on another device. Local edits have been kept. Resolve the conflict in project settings."),
            StatusCode::UNAUTHORIZED => error("Your server session has expired. Sign in again."),
            StatusCode::FORBIDDEN => error("Your team role does not allow this operation."),
            status => format!("Project sync returned HTTP {}. Try syncing again.",status.as_u16()),
        });
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = r
        .chunk()
        .await
        .map_err(|_| error("Project sync was interrupted. Try again."))?
    {
        if bytes.len() + chunk.len() > 3 * 1024 * 1024 {
            return Err(error("The server returned an oversized project record."));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| error("The server returned an unsupported project record."))
}
async fn latest(c: &Context, id: &str, mut cursor: i64) -> Result<(Option<Change>, i64), String> {
    let mut last = None;
    for _ in 0..10_000 {
        let path = format!("{}/changes?cursor={cursor}&limit=1", c.project_path(id));
        let page: Page = sync_json(
            client()?
                .get(c.url(&path)?)
                .headers(c.headers()?)
                .send()
                .await
                .map_err(|_| error("The sync server could not be reached."))?,
        )
        .await?;
        if page.changes.is_empty() {
            return Ok((last, cursor));
        }
        if page.next_cursor <= cursor {
            return Err(error("The server returned an invalid sync cursor."));
        }
        for change in page.changes {
            if change.cursor > page.next_cursor {
                return Err(error("Invalid project change cursor."));
            }
            if change.record_type == "project" && change.record_id == id {
                last = Some(change);
            }
        }
        cursor = page.next_cursor;
    }
    Err(error("Project history is too large to sync in one pass."))
}
async fn upload_files(
    c: &Context,
    ws: &Workspace,
    snapshot: &ProjectSnapshot,
) -> Result<(), String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Receipt {
        content_hash: String,
    }
    let path = c.project_path(&snapshot.project_id);
    let known: Vec<Receipt> = sync_json(
        client()?
            .get(c.url(&format!("{path}/blobs"))?)
            .headers(c.headers()?)
            .send()
            .await
            .map_err(|_| error("Could not check project files."))?,
    )
    .await?;
    let known = known
        .into_iter()
        .map(|receipt| receipt.content_hash)
        .collect::<HashSet<_>>();
    let mut pending = BTreeMap::<String, SyncBlob>::new();
    for blob in &snapshot.blobs {
        if !known.contains(&blob.hash) {
            pending
                .entry(blob.hash.clone())
                .or_insert_with(|| blob.clone());
        }
    }
    let http = transfer_client()?;
    let project_id = snapshot.project_id.clone();
    let memory_budget =
        std::sync::Arc::new(tokio::sync::Semaphore::new(BLOB_UPLOAD_MEMORY_BUDGET_MIB));
    stream::iter(pending.into_values())
        .map(|blob: SyncBlob| {
            let memory_budget = memory_budget.clone();
            let http = http.clone();
            let project_id = project_id.clone();
            async move {
                let permit_count = if blob.size <= 64 * 1024 * 1024 {
                    blob.size.div_ceil(1024 * 1024).clamp(1, 64) as u32
                } else {
                    8
                };
                let _memory = memory_budget
                    .acquire_many_owned(permit_count)
                    .await
                    .map_err(|_| error("Project file upload was cancelled."))?;
                ensure_enabled(ws, &project_id)?;
                upload_project_blob(c, ws, &project_id, &blob, &http).await
            }
        })
        .buffer_unordered(MAX_CONCURRENT_BLOB_UPLOADS)
        .try_collect::<Vec<()>>()
        .await?;
    Ok(())
}

async fn upload_project_blob(
    c: &Context,
    ws: &Workspace,
    project_id: &str,
    blob: &SyncBlob,
    http: &Client,
) -> Result<(), String> {
    let path = c.project_path(project_id);
    let source = ws.sync_blob_path(&blob.hash).map_err(|e| e.to_string())?;
    let mut file = std::fs::File::open(source).map_err(|_| error("A project file is missing."))?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest).map_err(|_| error("Could not read a project file."))?;
    if format!("{:x}", digest.finalize()) != blob.hash {
        return Err(error("A local project file failed its integrity check."));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| error("Could not read a project file."))?;

    if blob.size <= 64 * 1024 * 1024 {
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|_| error("Could not read a project file."))?;
        let _: Value = sync_json(
            http.put(c.url(&format!("{path}/blobs/{}", blob.hash))?)
                .headers(c.headers()?)
                .body(bytes)
                .send()
                .await
                .map_err(|_| error("A project file upload was interrupted."))?,
        )
        .await?;
        return Ok(());
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Upload {
        upload_id: Option<String>,
        already_present: bool,
        chunk_size_bytes: usize,
    }
    let upload: Upload = sync_json(
        http.post(c.url(&format!("{path}/blob-uploads"))?)
            .headers(c.headers()?)
            .json(&json!({"contentHash":blob.hash,"totalBytes":blob.size}))
            .send()
            .await
            .map_err(|_| error("Could not start a project file upload."))?,
    )
    .await?;
    if upload.already_present {
        return Ok(());
    }
    if upload.chunk_size_bytes == 0 || upload.chunk_size_bytes > 8 * 1024 * 1024 {
        return Err(error("Unsupported server upload chunk size."));
    }
    let upload_id = uuid::Uuid::parse_str(upload.upload_id.as_deref().unwrap_or(""))
        .map_err(|_| error("Invalid file upload session."))?;
    let mut index = 0;
    loop {
        ensure_enabled(ws, project_id)?;
        let mut bytes = vec![0; upload.chunk_size_bytes];
        let mut count = 0;
        while count < bytes.len() {
            let n = file
                .read(&mut bytes[count..])
                .map_err(|_| error("Could not read a project file."))?;
            if n == 0 {
                break;
            }
            count += n;
        }
        if count == 0 {
            break;
        }
        bytes.truncate(count);
        let _: Value = sync_json(
            http.put(c.url(&format!("{path}/blob-uploads/{upload_id}/chunks/{index}"))?)
                .headers(c.headers()?)
                .body(bytes)
                .send()
                .await
                .map_err(|_| {
                    error("A project file upload was interrupted. Retry sync to resume.")
                })?,
        )
        .await?;
        index += 1;
    }
    let _: Value = sync_json(
        http.post(c.url(&format!("{path}/blob-uploads/{upload_id}/complete"))?)
            .headers(c.headers()?)
            .send()
            .await
            .map_err(|_| error("Could not finish a project file upload."))?,
    )
    .await?;
    Ok(())
}
async fn download_files(
    c: &Context,
    ws: &Workspace,
    snapshot: &ProjectSnapshot,
) -> Result<(), String> {
    snapshot.validate().map_err(|e| e.to_string())?;
    for blob in &snapshot.blobs {
        if blob.remote_only {
            continue;
        }
        if blob.size > 512 * 1024 * 1024 {
            return Err(error("A server project file exceeds the 512 MB limit."));
        }
        let path = ws.sync_blob_path(&blob.hash).map_err(|e| e.to_string())?;
        if path.exists() {
            continue;
        }
        let mut response = transfer_client()?
            .get(c.url(&format!(
                "{}/blobs/{}",
                c.project_path(&snapshot.project_id),
                blob.hash
            ))?)
            .headers(c.headers()?)
            .send()
            .await
            .map_err(|_| error("Could not download a project file."))?;
        if !response.status().is_success() {
            return Err(error("A server project file is unavailable."));
        }
        let mut file =
            tempfile::NamedTempFile::new().map_err(|_| error("Could not stage a project file."))?;
        let mut size = 0u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| error("A project file download was interrupted."))?
        {
            size += chunk.len() as u64;
            if size > blob.size {
                return Err(error("A server file exceeds its declared size."));
            }
            file.write_all(&chunk)
                .map_err(|_| error("Could not save a downloaded project file."))?;
        }
        if size != blob.size {
            return Err(error("A project file download is incomplete."));
        }
        ws.receive_sync_blob(file.path(), &blob.hash)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
async fn sync_one(
    c: &Context,
    ws: &Workspace,
    id: &str,
    binding: Option<&ProjectSync>,
    report: &mut SyncReport,
) -> Result<SyncOutcome, String> {
    uuid::Uuid::parse_str(id).map_err(|_| error("Invalid server project ID."))?;
    let (remote, cursor) = latest(c, id, binding.map_or(0, |b| b.cursor)).await?;
    let local = match ws.project_snapshot(id) {
        Ok(p) => Some(p),
        Err(apex_core::Error::NotFound) => None,
        Err(e) => return Err(e.to_string()),
    };
    let local_name = local
        .as_ref()
        .map(ProjectSnapshot::name)
        .transpose()
        .map_err(|e| e.to_string())?;
    let local_hash = local
        .as_ref()
        .map(ProjectSnapshot::fingerprint)
        .transpose()
        .map_err(|e| e.to_string())?;
    let remote_snapshot = remote
        .as_ref()
        .filter(|r| !r.deleted)
        .and_then(|r| r.data.as_ref())
        .and_then(|d| d.get("apexSnapshot"))
        .map(|v| serde_json::from_value::<ProjectSnapshot>(v.clone()))
        .transpose()
        .map_err(|_| error("Unsupported project snapshot."))?;
    if let Some(r) = &remote {
        if r.deleted {
            return Err(error("This project was removed on the server. Your local copy has been kept; disable sync to keep working locally."));
        }
        if let Some(snapshot) = &remote_snapshot {
            if snapshot.project_id != id {
                return Err(error("The server returned a different project."));
            }
            let hash = snapshot.fingerprint().map_err(|e| e.to_string())?;
            if local_hash.as_deref() == Some(&hash) {
                sync_shared_decal_codes(c, ws, id).await?;
                ws.acknowledge_project_sync(id, &hash, r.revision, cursor)
                    .map_err(|e| e.to_string())?;
                return Ok(SyncOutcome::UpToDate);
            }
            let changed = binding.is_none_or(|b| r.revision != b.revision);
            if changed {
                if local.is_some()
                    && binding.and_then(|b| b.local_hash.as_ref()) != local_hash.as_ref()
                {
                    return Err(error("Conflict: both this device and the server changed. Local edits have been kept. Resolve the conflict in project settings."));
                }
                let mut incoming = snapshot.clone();
                incoming.classify_server_only_liveries();
                download_files(c, ws, &incoming).await?;
                if !c.can_apply() {
                    return Err(error("Finish editing before applying team updates."));
                }
                ws.apply_project_snapshot(
                    &incoming,
                    local_hash.as_deref(),
                    (
                        &origin_text(&c.base),
                        &c.server.server_instance_id,
                        &c.team.id,
                    ),
                    r.revision,
                )
                .map_err(|e| e.to_string())?;
                sync_shared_decal_codes(c, ws, id).await?;
                ws.acknowledge_project_sync(id, &hash, r.revision, cursor)
                    .map_err(|e| e.to_string())?;
                report.downloaded += 1;
                return Ok(SyncOutcome::Downloaded);
            }
        } else {
            let owned_local_project = binding.is_some_and(|binding| !binding.received_from_team);
            let recognized_placeholder = r.revision == 1
                && owned_local_project
                && local_name.as_deref().is_some_and(|name| {
                    r.data.as_ref().is_some_and(|data| {
                        let Some(object) = data.as_object() else {
                            return false;
                        };
                        object.get("name").and_then(serde_json::Value::as_str) == Some(name)
                            && (object.len() == 1
                                || (object.len() == 2
                                    && object.get("apexPendingSnapshot")
                                        == Some(&serde_json::Value::Bool(true))))
                    })
                });
            if !recognized_placeholder {
                return Ok(SyncOutcome::Skipped(
                    "The server project has no compatible Apex snapshot.",
                ));
            }
        } // Unknown server formats stay untouched; only Apex's own pending placeholder can be completed.
    }
    let Some(local) = local else {
        return Ok(SyncOutcome::Skipped(
            "No local project or compatible server snapshot was found.",
        ));
    };
    let hash = local_hash.ok_or_else(|| error("Local project is unavailable."))?;
    if remote_snapshot.is_some() && binding.and_then(|b| b.local_hash.as_ref()) == Some(&hash) {
        sync_shared_decal_codes(c, ws, id).await?;
        return Ok(SyncOutcome::UpToDate);
    }
    if c.team.role == "read_only" {
        return Err(error(
            "Read-only team access: local changes have not been uploaded.",
        ));
    }
    upload_files(c, ws, &local).await?;
    sync_shared_decal_codes(c, ws, id).await?;
    ensure_enabled(ws, id)?;
    let data = json!({"name":local.name().map_err(|e|e.to_string())?,"apexSnapshot":local});
    if serde_json::to_vec(&data)
        .map_err(|_| error("Could not encode project."))?
        .len()
        > 2 * 1024 * 1024
    {
        return Err(error(
            "Project metadata exceeds the server's 2 MB record limit.",
        ));
    }
    let base_revision = remote
        .as_ref()
        .map(|r| r.revision)
        .or_else(|| binding.map(|b| b.revision))
        .filter(|r| *r > 0);
    let revision: Revision = sync_json(
        client()?
            .put(c.url(&format!("{}/records/project/{id}", c.project_path(id)))?)
            .headers(c.headers()?)
            .json(&json!({"baseRevision":base_revision,"data":data}))
            .send()
            .await
            .map_err(|_| error("Project sync was interrupted."))?,
    )
    .await?;
    ws.acknowledge_project_sync(id, &hash, revision.revision, cursor)
        .map_err(|e| e.to_string())?;
    report.uploaded += 1;
    Ok(SyncOutcome::Uploaded)
}
pub async fn sync_projects(
    server_url: &str,
    team_id: &str,
    workspace: &Workspace,
) -> Result<SyncReport, String> {
    sync_projects_guarded(server_url, team_id, workspace, Arc::new(|| true)).await
}

pub async fn sync_projects_guarded(
    server_url: &str,
    team_id: &str,
    workspace: &Workspace,
    can_apply: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<SyncReport, String> {
    let _worker = worker_lock().lock().await;
    let ws = workspace.clone();
    let mut c = {
        match context(server_url, team_id).await {
            Ok(context) => context,
            Err(message) => {
                if message.contains("not available to this account")
                    || message.contains("session has expired")
                {
                    let _ = ws.clear_shared_decal_codes(team_id);
                }
                return Err(message);
            }
        }
    };
    c.can_apply = can_apply;
    if !c.can_apply() {
        return Err(error("Finish editing before synchronizing projects."));
    }
    let bindings = ws.project_sync_bindings().map_err(|e| e.to_string())?;
    let origin = origin_text(&c.base);
    let projects: Vec<RemoteProject> = sync_json(
        client()?
            .get(c.url(&format!("api/v1/teams/{}/projects", c.team.id))?)
            .headers(c.headers()?)
            .send()
            .await
            .map_err(|_| error("The sync server could not be reached."))?,
    )
    .await?;
    let mut report = SyncReport {
        shared_local: bindings
            .iter()
            .filter(|b| {
                b.enabled
                    && b.origin == origin
                    && b.server_instance == c.server.server_instance_id
                    && b.team_id == c.team.id
            })
            .count(),
        remote_projects: projects.len(),
        ..SyncReport::default()
    };
    let mut ids = projects.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    for b in &bindings {
        if b.origin == origin
            && b.server_instance == c.server.server_instance_id
            && b.team_id == c.team.id
            && b.enabled
            && !ids.contains(&b.project_id)
        {
            let local = ws
                .project_snapshot(&b.project_id)
                .map_err(|e| e.to_string())?;
            if b.revision > 0 {
                let message = "The server project is unavailable. Local data has been kept.";
                ws.sync_message(&b.project_id, message)
                    .map_err(|e| e.to_string())?;
                report.errors.push(format!(
                    "{}: {message}",
                    local.name().map_err(|e| e.to_string())?
                ));
                report.entries.push(SyncEntry {
                    project_id: b.project_id.clone(),
                    project_name: local.name().map_err(|e| e.to_string())?,
                    status: "error",
                    message: message.into(),
                });
                continue;
            }
            let response=client()?.post(c.url(&format!("api/v1/teams/{}/projects",c.team.id))?).headers(c.headers()?).json(&json!({"id":b.project_id,"data":{"name":local.name().map_err(|e|e.to_string())?,"apexPendingSnapshot":true}})).send().await.map_err(|_|error("Could not create the shared project."))?;
            if response.status() != StatusCode::CONFLICT {
                let _: Revision = sync_json(response).await?;
            }
            ids.push(b.project_id.clone());
        }
    }
    for id in ids {
        let binding = bindings.iter().find(|b| b.project_id == id);
        let name = ws.project(&id).map(|p| p.fields.name).unwrap_or_else(|_| {
            projects
                .iter()
                .find(|project| project.id == id)
                .map(|project| project.name.clone())
                .unwrap_or_else(|| id.clone())
        });
        let mismatch = binding.and_then(|binding| {
            if binding.origin != origin || binding.team_id != c.team.id {
                Some("This local project is linked to a different team or server instance. Use Reconnect to current team if this copy should join the selected team.")
            } else if binding.server_instance != c.server.server_instance_id {
                Some("This local project is linked to an older server instance. Use Reconnect to current team to check the current team copy.")
            } else if !binding.enabled {
                Some("Synchronization is disabled for this local project. It was left unchanged.")
            } else {
                None
            }
        });
        if let Some(message) = mismatch {
            ws.sync_message(&id, message).map_err(|e| e.to_string())?;
            report.entries.push(SyncEntry {
                project_id: id,
                project_name: name,
                status: "skipped",
                message: message.into(),
            });
            continue;
        }
        report.checked += 1;
        match sync_one(&c, &ws, &id, binding, &mut report).await {
            Ok(outcome) => {
                let (status, message) = match outcome {
                    SyncOutcome::Uploaded => ("uploaded", "Local changes uploaded."),
                    SyncOutcome::Downloaded => ("downloaded", "Server changes downloaded."),
                    SyncOutcome::UpToDate => {
                        report.up_to_date += 1;
                        ("up_to_date", "Already up to date.")
                    }
                    SyncOutcome::Skipped(reason) => ("skipped", reason),
                };
                report.entries.push(SyncEntry {
                    project_id: id,
                    project_name: name,
                    status,
                    message: message.into(),
                });
            }
            Err(message) => {
                ws.sync_message(&id, &message).map_err(|e| e.to_string())?;
                report.errors.push(format!("{name}: {message}"));
                report.entries.push(SyncEntry {
                    project_id: id,
                    project_name: name,
                    status: "error",
                    message,
                });
            }
        }
    }
    Ok(report)
}

pub async fn resolve_project_sync(
    project_id: &str,
    use_server: bool,
    workspace: &Workspace,
) -> Result<(), String> {
    let _worker = worker_lock().lock().await;
    let ws = workspace;
    let binding = ws
        .project_sync(project_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| error("This project is not linked to a server."))?;
    if !binding.enabled {
        return Err(error("Enable synchronization before resolving a conflict."));
    }
    let c = context(&binding.origin, &binding.team_id).await?;
    if c.server.server_instance_id != binding.server_instance {
        return Err(error(
            "This is a different server instance. Local data has been kept.",
        ));
    }
    let (remote, cursor) = latest(&c, project_id, 0).await?;
    let remote = remote
        .filter(|r| !r.deleted)
        .ok_or_else(|| error("The server project is unavailable."))?;
    let local = ws.project_snapshot(project_id).map_err(|e| e.to_string())?;
    let hash = local.fingerprint().map_err(|e| e.to_string())?;
    if use_server {
        let incoming: ProjectSnapshot = serde_json::from_value(
            remote
                .data
                .and_then(|d| d.get("apexSnapshot").cloned())
                .ok_or_else(|| error("The server project has no supported snapshot."))?,
        )
        .map_err(|_| error("Unsupported project snapshot."))?;
        if incoming.project_id != project_id {
            return Err(error("The server returned a different project."));
        }
        download_files(&c, ws, &incoming).await?;
        // The backup preserves the full local workspace before an explicit conflict resolution.
        ws.backup_before_sync_resolution()
            .map_err(|e| e.to_string())?;
        ws.apply_project_snapshot(
            &incoming,
            Some(&hash),
            (&binding.origin, &binding.server_instance, &binding.team_id),
            remote.revision,
        )
        .map_err(|e| e.to_string())?;
        ws.acknowledge_project_sync(
            project_id,
            &incoming.fingerprint().map_err(|e| e.to_string())?,
            remote.revision,
            cursor,
        )
        .map_err(|e| e.to_string())?;
    } else {
        if c.team.role == "read_only" {
            return Err(error(
                "Read-only members cannot replace the server version.",
            ));
        }
        upload_files(&c, ws, &local).await?;
        let revision:Revision=sync_json(client()?.put(c.url(&format!("{}/records/project/{project_id}",c.project_path(project_id)))?).headers(c.headers()?).json(&json!({"baseRevision":remote.revision,"data":{"name":local.name().map_err(|e|e.to_string())?,"apexSnapshot":local}})).send().await.map_err(|_|error("Could not resolve the server conflict."))?).await?;
        ws.acknowledge_project_sync(project_id, &hash, revision.revision, cursor)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        net::{TcpListener, TcpStream},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
        thread,
    };

    #[derive(Default)]
    struct Remote {
        revision: i64,
        events: Vec<Value>,
        blobs: HashMap<String, Vec<u8>>,
        reject_write: bool,
        upload_hash: String,
        chunks: std::collections::BTreeMap<u32, Vec<u8>>,
    }
    struct Server {
        base: String,
        remote: Arc<Mutex<Remote>>,
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
    }
    impl Server {
        fn new(id: &str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let remote = Arc::new(Mutex::new(Remote {
                revision: 1,
                events: vec![
                    json!({"cursor":1,"recordType":"project","recordId":id,"revision":1,"data":{"name":"Shared design"},"deleted":false}),
                ],
                ..Default::default()
            }));
            let state = remote.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let done = stop.clone();
            let id = id.to_owned();
            let thread = thread::spawn(move || {
                for connection in listener.incoming() {
                    if done.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut stream = connection.unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut buffer = [0u8; 8192];
                    let split = loop {
                        let n = stream.read(&mut buffer).unwrap();
                        if n == 0 {
                            break None;
                        }
                        bytes.extend_from_slice(&buffer[..n]);
                        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                            break Some(i + 4);
                        }
                    };
                    let Some(split) = split else {
                        continue;
                    };
                    let head = String::from_utf8(bytes[..split].to_vec()).unwrap();
                    assert!(head
                        .to_lowercase()
                        .contains("authorization: bearer fixture-session"));
                    let length = head
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    while bytes.len() < split + length {
                        let n = stream.read(&mut buffer).unwrap();
                        assert!(n > 0);
                        bytes.extend_from_slice(&buffer[..n]);
                    }
                    let mut request = head.lines().next().unwrap().split_whitespace();
                    let method = request.next().unwrap();
                    let path = request.next().unwrap();
                    let body = &bytes[split..split + length];
                    let mut r = state.lock().unwrap();
                    let mut status = 200;
                    let response = if path.contains("/changes?") {
                        let cursor = path
                            .split("cursor=")
                            .nth(1)
                            .unwrap()
                            .split('&')
                            .next()
                            .unwrap()
                            .parse::<i64>()
                            .unwrap();
                        let events = r
                            .events
                            .iter()
                            .filter(|e| e["cursor"].as_i64().unwrap() > cursor)
                            .take(1)
                            .cloned()
                            .collect::<Vec<_>>();
                        let next = events
                            .last()
                            .and_then(|e| e["cursor"].as_i64())
                            .unwrap_or(cursor);
                        serde_json::to_vec(&json!({"changes":events,"nextCursor":next})).unwrap()
                    } else if path.ends_with("/blobs") {
                        serde_json::to_vec(
                            &r.blobs
                                .keys()
                                .map(|h| json!({"contentHash":h}))
                                .collect::<Vec<_>>(),
                        )
                        .unwrap()
                    } else if path.ends_with("/blob-uploads") {
                        let value: Value = serde_json::from_slice(body).unwrap();
                        r.upload_hash = value["contentHash"].as_str().unwrap().into();
                        serde_json::to_vec(&json!({"uploadId":"019a0000-0000-7000-8000-000000000001","alreadyPresent":false,"chunkSizeBytes":8*1024*1024})).unwrap()
                    } else if path.contains("/chunks/") {
                        let index = path.rsplit('/').next().unwrap().parse().unwrap();
                        r.chunks.insert(index, body.to_vec());
                        status = 204;
                        Vec::new()
                    } else if path.ends_with("/complete") {
                        let bytes = r.chunks.values().flatten().copied().collect::<Vec<_>>();
                        let hash = r.upload_hash.clone();
                        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), hash);
                        let size = bytes.len();
                        r.blobs.insert(hash.clone(), bytes);
                        serde_json::to_vec(&json!({"contentHash":hash,"byteSize":size})).unwrap()
                    } else if path.contains("/blobs/") {
                        let hash = path.rsplit('/').next().unwrap();
                        if method == "PUT" {
                            assert_eq!(format!("{:x}", Sha256::digest(body)), hash);
                            r.blobs.insert(hash.into(), body.to_vec());
                            serde_json::to_vec(&json!({"contentHash":hash,"byteSize":body.len()}))
                                .unwrap()
                        } else {
                            r.blobs.get(hash).unwrap().clone()
                        }
                    } else if path.contains("/records/project/") {
                        let value: Value = serde_json::from_slice(body).unwrap();
                        if r.reject_write || value["baseRevision"] != r.revision {
                            r.reject_write = false;
                            status = 409;
                            serde_json::to_vec(&json!({"error":"revision_conflict"})).unwrap()
                        } else {
                            r.revision += 1;
                            let rev = r.revision;
                            r.events.push(json!({"cursor":rev,"recordType":"project","recordId":id,"revision":rev,"data":value["data"],"deleted":false}));
                            serde_json::to_vec(&json!({"revision":rev})).unwrap()
                        }
                    } else {
                        panic!("Unexpected fixture request: {method} {path}");
                    };
                    drop(r);
                    write!(stream,"HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",response.len()).unwrap();
                    stream.write_all(&response).unwrap();
                }
            });
            Self {
                base,
                remote,
                stop,
                thread: Some(thread),
            }
        }
        fn context(&self) -> Context {
            Context {
                base: Url::parse(&self.base).unwrap(),
                session: SavedSession {
                    server_origin: self.base.clone(),
                    server_instance_id: "fixture".into(),
                    access_token: "fixture-session".into(),
                },
                server: SyncServerInfo {
                    product: "Apex Sync Server".into(),
                    api_version: "v1".into(),
                    server_version: "fixture".into(),
                    server_instance_id: "fixture".into(),
                    origin: self.base.clone(),
                },
                team: SyncTeam {
                    id: "team".into(),
                    name: "Test team".into(),
                    role: "designer".into(),
                },
                can_apply: Arc::new(|| true),
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let _ = TcpStream::connect(self.base.trim_start_matches("http://"));
            if let Some(t) = self.thread.take() {
                t.join().unwrap();
            }
        }
    }

    #[test]
    fn large_files_use_chunks_and_accept_no_content_receipts() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let temp = tempfile::tempdir().unwrap();
                let ws = Workspace::open(temp.path().join("ws")).unwrap();
                let project = ws
                    .save_project(
                        None,
                        apex_core::ProjectInput {
                            name: "Large artwork".into(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                let source = temp.path().join("large.psd");
                std::fs::File::create(&source)
                    .unwrap()
                    .set_len(65 * 1024 * 1024)
                    .unwrap();
                ws.import_project_files(&project.id, &[source]).unwrap();
                let server = Server::new(&project.id);
                ws.configure_project_sync(&project.id, Some((&server.base, "fixture", "team")))
                    .unwrap();
                let snapshot = ws.project_snapshot(&project.id).unwrap();
                upload_files(&server.context(), &ws, &snapshot)
                    .await
                    .unwrap();
                let remote = server.remote.lock().unwrap();
                assert_eq!(remote.chunks.len(), 9);
                assert_eq!(
                    remote.blobs[&snapshot.blobs[0].hash].len(),
                    65 * 1024 * 1024
                );
            });
    }
    #[test]
    fn two_devices_exchange_files_and_edits_and_preserve_conflicts() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let temp = tempfile::tempdir().unwrap();
                let a = Workspace::open(temp.path().join("a")).unwrap();
                let b = Workspace::open(temp.path().join("b")).unwrap();
                let project = a
                    .save_project(
                        None,
                        apex_core::ProjectInput {
                            name: "Shared design".into(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                let source = temp.path().join("design.psd");
                std::fs::write(&source, b"source artwork fixture").unwrap();
                a.import_project_files(&project.id, &[source]).unwrap();
                let server = Server::new(&project.id);
                let c = server.context();
                a.configure_project_sync(&project.id, Some((&server.base, "fixture", "team")))
                    .unwrap();
                let mut report = SyncReport::default();
                sync_one(
                    &c,
                    &a,
                    &project.id,
                    a.project_sync(&project.id).unwrap().as_ref(),
                    &mut report,
                )
                .await
                .unwrap();
                assert_eq!(report.uploaded, 1);
                let unchanged = sync_one(
                    &c,
                    &a,
                    &project.id,
                    a.project_sync(&project.id).unwrap().as_ref(),
                    &mut report,
                )
                .await
                .unwrap();
                assert!(matches!(unchanged, SyncOutcome::UpToDate));
                sync_one(&c, &b, &project.id, None, &mut report)
                    .await
                    .unwrap();
                assert_eq!(report.downloaded, 1);
                assert_eq!(
                    a.project_snapshot(&project.id).unwrap(),
                    b.project_snapshot(&project.id).unwrap()
                );
                let mut fields = project.fields.clone();
                fields.description = "Edit on A".into();
                a.save_project(Some(&project.id), fields).unwrap();
                sync_one(
                    &c,
                    &a,
                    &project.id,
                    a.project_sync(&project.id).unwrap().as_ref(),
                    &mut report,
                )
                .await
                .unwrap();
                sync_one(
                    &c,
                    &b,
                    &project.id,
                    b.project_sync(&project.id).unwrap().as_ref(),
                    &mut report,
                )
                .await
                .unwrap();
                assert_eq!(
                    b.project(&project.id).unwrap().fields.description,
                    "Edit on A"
                );
                for (ws, name) in [(&a, "A changes again"), (&b, "B local change")] {
                    let mut fields = project.fields.clone();
                    fields.description = name.into();
                    ws.save_project(Some(&project.id), fields).unwrap();
                }
                sync_one(
                    &c,
                    &a,
                    &project.id,
                    a.project_sync(&project.id).unwrap().as_ref(),
                    &mut report,
                )
                .await
                .unwrap();
                let error = sync_one(
                    &c,
                    &b,
                    &project.id,
                    b.project_sync(&project.id).unwrap().as_ref(),
                    &mut report,
                )
                .await
                .unwrap_err();
                assert!(error.starts_with("Conflict:"));
                assert_eq!(
                    b.project(&project.id).unwrap().fields.description,
                    "B local change"
                );
                let mut fields = project.fields.clone();
                fields.description = "Retained after rejected upload".into();
                a.save_project(Some(&project.id), fields).unwrap();
                server.remote.lock().unwrap().reject_write = true;
                assert!(sync_one(
                    &c,
                    &a,
                    &project.id,
                    a.project_sync(&project.id).unwrap().as_ref(),
                    &mut report
                )
                .await
                .unwrap_err()
                .starts_with("Conflict:"));
                assert_eq!(
                    a.project(&project.id).unwrap().fields.description,
                    "Retained after rejected upload"
                );
            });
    }
}
