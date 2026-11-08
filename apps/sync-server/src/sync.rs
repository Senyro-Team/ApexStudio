use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::{get, post, put},
    Json, Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{
    auth::{membership_role, ApiError, AuthUser},
    AppState,
};

const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;
const MAX_CHANGE_PAGE: i64 = 500;
const MAX_DECAL_CODE_BATCH: usize = 5_000;
const MAX_BLOB_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DIRECT_BLOB_BYTES: u64 = 64 * 1024 * 1024;
const BLOB_CHUNK_BYTES: usize = 8 * 1024 * 1024;
const MAX_PENDING_BLOB_UPLOADS: i64 = 8;
const MAX_PENDING_BLOB_UPLOAD_BYTES: i64 = 2 * 1024 * 1024 * 1024;
pub(crate) const BLOB_UPLOAD_ADVISORY_LOCK: i64 = 0x4150455855504C44;

#[derive(Deserialize)]
struct CreateProjectRequest {
    id: Uuid,
    data: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    id: Uuid,
    name: String,
    revision: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRecordRequest {
    base_revision: Option<i64>,
    data: Option<Value>,
    #[serde(default)]
    deleted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordRevision {
    revision: i64,
}

#[derive(Deserialize)]
struct ChangeQuery {
    #[serde(default)]
    cursor: i64,
    #[serde(default = "default_page_size")]
    limit: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectChange {
    cursor: i64,
    record_type: String,
    record_id: Uuid,
    revision: i64,
    data: Option<Value>,
    deleted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangesPage {
    changes: Vec<ProjectChange>,
    next_cursor: i64,
}

struct RecordWrite {
    team_id: Uuid,
    project_id: Uuid,
    record_type: String,
    record_id: Uuid,
    base_revision: Option<i64>,
    data: Option<Value>,
    deleted: bool,
    actor: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlobReceipt {
    content_hash: String,
    byte_size: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartBlobUploadRequest {
    content_hash: String,
    total_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartBlobUploadResponse {
    upload_id: Option<Uuid>,
    already_present: bool,
    chunk_size_bytes: usize,
    total_chunks: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlobUploadStatus {
    upload_id: Uuid,
    content_hash: String,
    total_bytes: u64,
    uploaded_chunks: Vec<u32>,
    total_chunks: u32,
    complete: bool,
}

fn default_page_size() -> i64 {
    200
}

pub fn private_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/teams/{team_id}/decal-codes/{content_hash}",
            get(get_team_decal_code).put(save_team_decal_code),
        )
        .route(
            "/api/v1/teams/{team_id}/decal-codes/batch",
            post(batch_team_decal_codes),
        )
        .route(
            "/api/v1/teams/{team_id}/projects",
            get(list_projects).post(create_project),
        )
        .route(
            "/api/v1/teams/{team_id}/projects/{project_id}/records/{record_type}/{record_id}",
            put(update_record),
        )
        .route(
            "/api/v1/teams/{team_id}/projects/{project_id}/changes",
            get(list_changes),
        )
        .route(
            "/api/v1/teams/{team_id}/projects/{project_id}/blobs/{content_hash}",
            put(upload_blob).get(download_blob),
        )
        .route(
            "/api/v1/teams/{team_id}/projects/{project_id}/blobs",
            get(list_blobs),
        )
        .route(
            "/api/v1/teams/{team_id}/projects/{project_id}/blob-uploads",
            post(start_blob_upload),
        )
        .route(
            "/api/v1/teams/{team_id}/projects/{project_id}/blob-uploads/{upload_id}",
            get(blob_upload_status).delete(cancel_blob_upload),
        )
        .route(
            "/api/v1/teams/{team_id}/projects/{project_id}/blob-uploads/{upload_id}/chunks/{chunk_index}",
            put(upload_blob_chunk),
        )
        .route(
            "/api/v1/teams/{team_id}/projects/{project_id}/blob-uploads/{upload_id}/complete",
            post(complete_blob_upload),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TeamDecalCode {
    content_hash: String,
    decal_asset_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveTeamDecalCodeRequest {
    decal_asset_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchTeamDecalCodesRequest {
    codes: Vec<BatchTeamDecalCodeInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchTeamDecalCodeInput {
    content_hash: String,
    decal_asset_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchTeamDecalCodeResult {
    content_hash: String,
    decal_asset_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchTeamDecalCodesResponse {
    codes: Vec<BatchTeamDecalCodeResult>,
}

async fn get_team_decal_code(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, content_hash)): Path<(Uuid, String)>,
) -> Result<Json<Option<TeamDecalCode>>, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    validate_hash(&content_hash)?;
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT content_hash,decal_asset_id FROM team_decal_codes WHERE team_id=$1 AND content_hash=$2",
    )
    .bind(team_id)
    .bind(&content_hash)
    .fetch_optional(&state.database)
    .await?;
    Ok(Json(row.map(|(content_hash, decal_asset_id)| {
        TeamDecalCode {
            content_hash: content_hash.trim().to_owned(),
            decal_asset_id,
        }
    })))
}

async fn save_team_decal_code(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, content_hash)): Path<(Uuid, String)>,
    Json(input): Json<SaveTeamDecalCodeRequest>,
) -> Result<Json<TeamDecalCode>, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if !role.can_write() {
        return Err(ApiError::forbidden());
    }
    validate_hash(&content_hash)?;
    if input.decal_asset_id.is_empty()
        || input.decal_asset_id.len() > 20
        || input.decal_asset_id.starts_with('0')
        || !input
            .decal_asset_id
            .bytes()
            .all(|byte| byte.is_ascii_digit())
    {
        return Err(ApiError::bad_request("invalid_decal_asset_id"));
    }
    let inserted = sqlx::query_scalar::<_, String>(
        "INSERT INTO team_decal_codes(team_id,content_hash,decal_asset_id,uploaded_by) VALUES($1,$2,$3,$4) ON CONFLICT(team_id,content_hash) DO NOTHING RETURNING decal_asset_id",
    )
    .bind(team_id)
    .bind(&content_hash)
    .bind(&input.decal_asset_id)
    .bind(user.id)
    .fetch_optional(&state.database)
    .await?;
    let selected =
        match inserted {
            Some(id) => id,
            None => sqlx::query_scalar::<_, String>(
                "SELECT decal_asset_id FROM team_decal_codes WHERE team_id=$1 AND content_hash=$2",
            )
            .bind(team_id)
            .bind(&content_hash)
            .fetch_one(&state.database)
            .await?,
        };
    if selected != input.decal_asset_id {
        return Err(ApiError::conflict("team_decal_code_conflict"));
    }
    tracing::info!(event = "team_decal_code_shared", team_id = %team_id, actor_id = %user.id, "Roblox decal code shared with team");
    Ok(Json(TeamDecalCode {
        content_hash: content_hash.trim().to_owned(),
        decal_asset_id: selected,
    }))
}

async fn batch_team_decal_codes(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path(team_id): Path<Uuid>,
    Json(input): Json<BatchTeamDecalCodesRequest>,
) -> Result<Json<BatchTeamDecalCodesResponse>, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if input.codes.len() > MAX_DECAL_CODE_BATCH {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "decal_code_batch_too_large",
        ));
    }

    let mut requested_ids = std::collections::HashMap::<String, Option<String>>::new();
    for code in input.codes {
        validate_hash(&code.content_hash)?;
        if let Some(id) = &code.decal_asset_id {
            if id.is_empty()
                || id.len() > 20
                || id.starts_with('0')
                || !id.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(ApiError::bad_request("invalid_decal_asset_id"));
            }
            if !role.can_write() {
                return Err(ApiError::forbidden());
            }
        }
        if let Some(previous) = requested_ids.insert(code.content_hash, code.decal_asset_id.clone())
        {
            if previous != code.decal_asset_id {
                return Err(ApiError::bad_request("duplicate_decal_code_conflict"));
            }
        }
    }

    let hashes = requested_ids.keys().cloned().collect::<Vec<_>>();
    if hashes.is_empty() {
        return Ok(Json(BatchTeamDecalCodesResponse { codes: Vec::new() }));
    }

    let mut transaction = state.database.begin().await?;
    let write_codes = requested_ids
        .iter()
        .filter_map(|(hash, id)| id.as_ref().map(|id| (hash.clone(), id.clone())))
        .collect::<Vec<_>>();
    if !write_codes.is_empty() {
        let (write_hashes, write_ids): (Vec<_>, Vec<_>) = write_codes.into_iter().unzip();
        sqlx::query("INSERT INTO team_decal_codes(team_id,content_hash,decal_asset_id,uploaded_by) SELECT $1, batch.content_hash, batch.decal_asset_id, $4 FROM UNNEST($2::text[], $3::text[]) AS batch(content_hash,decal_asset_id) ON CONFLICT(team_id,content_hash) DO NOTHING")
            .bind(team_id)
            .bind(&write_hashes)
            .bind(&write_ids)
            .bind(user.id)
            .execute(&mut *transaction)
            .await?;
    }

    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT content_hash,decal_asset_id FROM team_decal_codes WHERE team_id=$1 AND content_hash=ANY($2)",
    )
    .bind(team_id)
    .bind(&hashes)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;

    let shared = rows
        .into_iter()
        .map(|(hash, id)| (hash.trim().to_owned(), id))
        .collect::<std::collections::HashMap<_, _>>();
    let codes = hashes
        .into_iter()
        .map(|content_hash| BatchTeamDecalCodeResult {
            decal_asset_id: shared.get(&content_hash).cloned(),
            content_hash,
        })
        .collect::<Vec<_>>();
    tracing::info!(event = "team_decal_codes_batch_synced", team_id = %team_id, actor_id = %user.id, code_count = codes.len(), "Team Roblox decal codes synchronized in batch");
    Ok(Json(BatchTeamDecalCodesResponse { codes }))
}

async fn create_project(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path(team_id): Path<Uuid>,
    Json(input): Json<CreateProjectRequest>,
) -> Result<Json<RecordRevision>, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if !role.can_write() {
        return Err(ApiError::forbidden());
    }
    let data = validate_record_data("project", input.data)?;
    let mut transaction = state.database.begin().await?;
    let revision = save_record(
        &mut transaction,
        RecordWrite {
            team_id,
            project_id: input.id,
            record_type: "project".into(),
            record_id: input.id,
            base_revision: None,
            data: Some(data),
            deleted: false,
            actor: user.id,
        },
    )
    .await?;
    transaction.commit().await?;
    tracing::info!(event = "project_created", team_id = %team_id, project_id = %input.id, actor_id = %user.id, revision, "Project record synchronized");
    Ok(Json(RecordRevision { revision }))
}

async fn list_projects(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path(team_id): Path<Uuid>,
) -> Result<Json<Vec<ProjectSummary>>, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    let rows = sqlx::query(
        "SELECT record_id, data->>'name' AS name, revision FROM sync_records WHERE team_id=$1 AND project_id=record_id AND record_type='project' AND deleted=FALSE ORDER BY lower(data->>'name'),record_id",
    )
    .bind(team_id)
    .fetch_all(&state.database)
    .await
    .map_err(ApiError::from)?;
    let projects = rows
        .into_iter()
        .map(|row| {
            Ok(ProjectSummary {
                id: row.try_get("record_id").map_err(ApiError::from)?,
                name: row.try_get("name").map_err(ApiError::from)?,
                revision: row.try_get("revision").map_err(ApiError::from)?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(projects))
}

async fn update_record(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, project_id, record_type, record_id)): Path<(Uuid, Uuid, String, Uuid)>,
    Json(input): Json<UpdateRecordRequest>,
) -> Result<Json<RecordRevision>, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if !role.can_write() {
        return Err(ApiError::forbidden());
    }
    validate_record_type(&record_type)?;
    let root_project = record_type == "project" && record_id == project_id;
    if record_type == "project" && !root_project {
        return Err(ApiError::bad_request("project_record_id_mismatch"));
    }
    if !root_project {
        ensure_project_exists(&state, team_id, project_id).await?;
    }
    if input.base_revision.is_some_and(|revision| revision < 1) {
        return Err(ApiError::bad_request("invalid_base_revision"));
    }
    if input.deleted && input.data.is_some() {
        return Err(ApiError::bad_request(
            "deleted_record_must_not_include_data",
        ));
    }
    let payload_bytes = input
        .data
        .as_ref()
        .map_or(0, |value| value.to_string().len());
    let data = if input.deleted {
        None
    } else {
        Some(validate_record_data(
            &record_type,
            input
                .data
                .ok_or_else(|| ApiError::bad_request("record_data_required"))?,
        )?)
    };

    let deleted = input.deleted;
    let logged_record_type = record_type.clone();
    let mut transaction = state.database.begin().await?;
    let revision = save_record(
        &mut transaction,
        RecordWrite {
            team_id,
            project_id,
            record_type,
            record_id,
            base_revision: input.base_revision,
            data,
            deleted: input.deleted,
            actor: user.id,
        },
    )
    .await?;
    transaction.commit().await?;
    tracing::info!(event = "project_record_synced", team_id = %team_id, project_id = %project_id, record_id = %record_id, record_type = %logged_record_type, actor_id = %user.id, revision, deleted, payload_bytes, "Project record synchronized");
    Ok(Json(RecordRevision { revision }))
}

async fn list_changes(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, project_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<ChangeQuery>,
) -> Result<Json<ChangesPage>, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    if query.cursor < 0 || !(1..=MAX_CHANGE_PAGE).contains(&query.limit) {
        return Err(ApiError::bad_request("invalid_change_cursor_or_limit"));
    }
    ensure_project_any_revision(&state, team_id, project_id).await?;
    let rows = sqlx::query(
        "SELECT cursor,record_type,record_id,revision,data,deleted FROM sync_events WHERE team_id=$1 AND project_id=$2 AND cursor>$3 ORDER BY cursor LIMIT $4",
    )
    .bind(team_id)
    .bind(project_id)
    .bind(query.cursor)
    .bind(query.limit)
    .fetch_all(&state.database)
    .await
    .map_err(ApiError::from)?;

    let changes = rows
        .into_iter()
        .map(|row| {
            Ok(ProjectChange {
                cursor: row.try_get("cursor").map_err(ApiError::from)?,
                record_type: row.try_get("record_type").map_err(ApiError::from)?,
                record_id: row.try_get("record_id").map_err(ApiError::from)?,
                revision: row.try_get("revision").map_err(ApiError::from)?,
                data: row.try_get("data").map_err(ApiError::from)?,
                deleted: row.try_get("deleted").map_err(ApiError::from)?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let next_cursor = changes.last().map_or(query.cursor, |change| change.cursor);
    if !changes.is_empty() {
        tracing::info!(event = "project_changes_served", team_id = %team_id, project_id = %project_id, actor_id = %user.id, change_count = changes.len(), from_cursor = query.cursor, next_cursor, "Project changes served");
    }
    Ok(Json(ChangesPage {
        changes,
        next_cursor,
    }))
}

async fn save_record(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    write: RecordWrite,
) -> Result<i64, ApiError> {
    let project_lock = format!("{}:{}", write.team_id, write.project_id);
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(project_lock)
        .execute(&mut **transaction)
        .await?;

    let current_revision = sqlx::query_scalar::<_, i64>(
        "SELECT revision FROM sync_records WHERE team_id=$1 AND project_id=$2 AND record_type=$3 AND record_id=$4 FOR UPDATE",
    )
    .bind(write.team_id)
    .bind(write.project_id)
    .bind(&write.record_type)
    .bind(write.record_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let revision = match (current_revision, write.base_revision) {
        (None, None) => 1,
        (Some(current), Some(base)) if current == base => current + 1,
        _ => return Err(ApiError::conflict("revision_conflict")),
    };

    sqlx::query(
        "INSERT INTO sync_records (team_id,project_id,record_type,record_id,revision,data,deleted,updated_by,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,now()) ON CONFLICT (team_id,project_id,record_type,record_id) DO UPDATE SET revision=EXCLUDED.revision,data=EXCLUDED.data,deleted=EXCLUDED.deleted,updated_by=EXCLUDED.updated_by,updated_at=now()",
    )
    .bind(write.team_id)
    .bind(write.project_id)
    .bind(&write.record_type)
    .bind(write.record_id)
    .bind(revision)
    .bind(write.data.as_ref())
    .bind(write.deleted)
    .bind(write.actor)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO sync_events (team_id,project_id,record_type,record_id,revision,data,deleted,actor_user_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(write.team_id)
    .bind(write.project_id)
    .bind(&write.record_type)
    .bind(write.record_id)
    .bind(revision)
    .bind(write.data.as_ref())
    .bind(write.deleted)
    .bind(write.actor)
    .execute(&mut **transaction)
    .await?;
    Ok(revision)
}

fn validate_record_type(value: &str) -> Result<(), ApiError> {
    if matches!(
        value,
        "project" | "note" | "folder" | "livery" | "asset" | "placement" | "post" | "post_media"
    ) {
        Ok(())
    } else {
        Err(ApiError::bad_request("unsupported_record_type"))
    }
}

fn validate_record_data(record_type: &str, mut data: Value) -> Result<Value, ApiError> {
    let bytes =
        serde_json::to_vec(&data).map_err(|_| ApiError::bad_request("invalid_record_data"))?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(ApiError::new(
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            "record_too_large",
        ));
    }
    let object = data
        .as_object_mut()
        .ok_or_else(|| ApiError::bad_request("record_must_be_json_object"))?;
    if record_type == "project" {
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::bad_request("project_name_required"))?;
        let name = name.trim();
        if !(2..=120).contains(&name.chars().count()) || name.chars().any(char::is_control) {
            return Err(ApiError::bad_request("invalid_project_name"));
        }
        object.insert("name".into(), Value::String(name.to_owned()));
    }
    Ok(data)
}

async fn ensure_project_exists(
    state: &AppState,
    team_id: Uuid,
    project_id: Uuid,
) -> Result<(), ApiError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM sync_records WHERE team_id=$1 AND project_id=$2 AND record_type='project' AND record_id=$2 AND deleted=FALSE)",
    )
    .bind(team_id)
    .bind(project_id)
    .fetch_one(&state.database)
    .await?;
    if exists {
        Ok(())
    } else {
        Err(ApiError::not_found("project_not_found"))
    }
}

async fn ensure_project_any_revision(
    state: &AppState,
    team_id: Uuid,
    project_id: Uuid,
) -> Result<(), ApiError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM sync_records WHERE team_id=$1 AND project_id=$2 AND record_type='project' AND record_id=$2)",
    )
    .bind(team_id)
    .bind(project_id)
    .fetch_one(&state.database)
    .await?;
    if exists {
        Ok(())
    } else {
        Err(ApiError::not_found("project_not_found"))
    }
}

async fn upload_blob(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, project_id, content_hash)): Path<(Uuid, Uuid, String)>,
    body: Body,
) -> Result<Json<BlobReceipt>, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if !role.can_write() {
        return Err(ApiError::forbidden());
    }
    validate_hash(&content_hash)?;
    ensure_project_exists(&state, team_id, project_id).await?;

    let staging_dir = state.storage_dir.join("staging");
    let blobs_dir = state.storage_dir.join("blobs");
    fs::create_dir_all(&staging_dir)
        .await
        .map_err(|_| ApiError::internal())?;
    let staged_path = staging_dir.join(Uuid::now_v7().to_string());
    let staged = StagedBlob(staged_path.clone());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_path)
        .await
        .map_err(|_| ApiError::internal())?;
    let mut hasher = Sha256::new();
    let mut byte_size = 0_u64;
    let mut stream = body.into_data_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::bad_request("invalid_blob_stream"))?;
        byte_size = byte_size
            .checked_add(chunk.len() as u64)
            .filter(|size| *size <= MAX_DIRECT_BLOB_BYTES)
            .ok_or_else(|| {
                ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "use_chunked_blob_upload")
            })?;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|_| ApiError::internal())?;
    }
    file.sync_all().await.map_err(|_| ApiError::internal())?;
    drop(file);

    let actual_hash = format!("{:x}", hasher.finalize());
    if actual_hash != content_hash {
        return Err(ApiError::bad_request("blob_hash_mismatch"));
    }

    let byte_size_i64 =
        i64::try_from(byte_size).map_err(|_| ApiError::bad_request("blob_too_large"))?;
    let mut transaction = state.database.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BLOB_UPLOAD_ADVISORY_LOCK)
        .execute(&mut *transaction)
        .await?;
    let (stored_bytes, pending_bytes) = blob_storage_reservations(&mut transaction).await?;
    if !has_blob_storage_capacity(
        stored_bytes,
        pending_bytes,
        byte_size_i64,
        state.max_blob_storage_bytes,
    ) {
        return Err(ApiError::new(
            StatusCode::INSUFFICIENT_STORAGE,
            "blob_storage_capacity_reached",
        ));
    }

    let prefix = &content_hash[..2];
    let blob_dir = blobs_dir.join(prefix);
    fs::create_dir_all(&blob_dir)
        .await
        .map_err(|_| ApiError::internal())?;
    let blob_path = blob_dir.join(&content_hash);
    fs::rename(&staged_path, &blob_path)
        .await
        .map_err(|_| ApiError::internal())?;
    drop(staged);

    sqlx::query("INSERT INTO project_blobs (team_id,project_id,content_hash,byte_size,uploaded_by) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (team_id,project_id,content_hash) DO UPDATE SET byte_size=EXCLUDED.byte_size,uploaded_by=EXCLUDED.uploaded_by")
        .bind(team_id)
        .bind(project_id)
        .bind(&content_hash)
        .bind(byte_size_i64)
        .bind(user.id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;

    tracing::info!(event = "project_blob_uploaded", team_id = %team_id, project_id = %project_id, actor_id = %user.id, byte_size, transfer = "direct", "Project blob upload completed");
    Ok(Json(BlobReceipt {
        content_hash,
        byte_size,
    }))
}

async fn download_blob(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, project_id, content_hash)): Path<(Uuid, Uuid, String)>,
) -> Result<Response, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    validate_hash(&content_hash)?;
    ensure_project_exists(&state, team_id, project_id).await?;
    let byte_size = sqlx::query_scalar::<_, i64>(
        "SELECT byte_size FROM project_blobs WHERE team_id=$1 AND project_id=$2 AND content_hash=$3",
    )
    .bind(team_id)
    .bind(project_id)
    .bind(&content_hash)
    .fetch_optional(&state.database)
    .await?
    .ok_or_else(|| ApiError::not_found("blob_not_found"))?;

    let path = state
        .storage_dir
        .join("blobs")
        .join(&content_hash[..2])
        .join(&content_hash);
    let file = File::open(path)
        .await
        .map_err(|_| ApiError::not_found("blob_not_found"))?;
    tracing::info!(event = "project_blob_download_started", team_id = %team_id, project_id = %project_id, actor_id = %user.id, byte_size, "Project blob download started");
    let mut response = Response::new(Body::from_stream(ReaderStream::new(file)));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&byte_size.to_string()).map_err(|_| ApiError::internal())?,
    );
    Ok(response)
}

async fn list_blobs(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<BlobReceipt>>, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    ensure_project_exists(&state, team_id, project_id).await?;
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT content_hash,byte_size FROM project_blobs WHERE team_id=$1 AND project_id=$2 ORDER BY content_hash",
    )
    .bind(team_id)
    .bind(project_id)
    .fetch_all(&state.database)
    .await?;
    let receipts = rows
        .into_iter()
        .map(|(content_hash, byte_size)| {
            Ok(BlobReceipt {
                content_hash: content_hash.trim().to_owned(),
                byte_size: u64::try_from(byte_size).map_err(|_| ApiError::internal())?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(receipts))
}

async fn start_blob_upload(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, project_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<StartBlobUploadRequest>,
) -> Result<Json<StartBlobUploadResponse>, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if !role.can_write() {
        return Err(ApiError::forbidden());
    }
    validate_hash(&input.content_hash)?;
    ensure_project_exists(&state, team_id, project_id).await?;
    if input.total_bytes > MAX_BLOB_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "blob_too_large",
        ));
    }
    if let Some(existing_size) = sqlx::query_scalar::<_, i64>(
        "SELECT byte_size FROM project_blobs WHERE team_id=$1 AND project_id=$2 AND content_hash=$3",
    )
    .bind(team_id)
    .bind(project_id)
    .bind(&input.content_hash)
    .fetch_optional(&state.database)
    .await?
    {
        if u64::try_from(existing_size).ok() != Some(input.total_bytes) {
            return Err(ApiError::conflict("blob_size_conflict"));
        }
        return Ok(Json(StartBlobUploadResponse {
            upload_id: None,
            already_present: true,
            chunk_size_bytes: BLOB_CHUNK_BYTES,
            total_chunks: chunk_count(input.total_bytes)?,
        }));
    }

    let total_bytes =
        i64::try_from(input.total_bytes).map_err(|_| ApiError::bad_request("invalid_blob_size"))?;
    let mut transaction = state.database.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BLOB_UPLOAD_ADVISORY_LOCK)
        .execute(&mut *transaction)
        .await?;
    let (pending_uploads, pending_bytes) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT COUNT(*)::BIGINT, COALESCE(SUM(total_bytes), 0)::BIGINT FROM blob_uploads WHERE completed_at IS NULL",
    )
    .fetch_one(&mut *transaction)
    .await?;
    let (stored_bytes, _) = blob_storage_reservations(&mut transaction).await?;
    if !has_upload_capacity(
        pending_uploads,
        pending_bytes,
        stored_bytes,
        total_bytes,
        state.max_blob_storage_bytes,
    ) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "upload_capacity_reached",
        ));
    }

    let upload_id = Uuid::now_v7();
    sqlx::query("INSERT INTO blob_uploads (id,team_id,project_id,content_hash,total_bytes,created_by,expires_at) VALUES ($1,$2,$3,$4,$5,$6,now()+interval '24 hours')")
        .bind(upload_id)
        .bind(team_id)
        .bind(project_id)
        .bind(&input.content_hash)
        .bind(total_bytes)
        .bind(user.id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;

    Ok(Json(StartBlobUploadResponse {
        upload_id: Some(upload_id),
        already_present: false,
        chunk_size_bytes: BLOB_CHUNK_BYTES,
        total_chunks: chunk_count(input.total_bytes)?,
    }))
}

async fn blob_storage_reservations(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(i64, i64), ApiError> {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT COALESCE((SELECT SUM(byte_size) FROM (SELECT content_hash, MAX(byte_size) AS byte_size FROM project_blobs GROUP BY content_hash) stored), 0)::BIGINT, COALESCE((SELECT SUM(total_bytes) FROM blob_uploads WHERE completed_at IS NULL), 0)::BIGINT",
    )
    .fetch_one(&mut **transaction)
    .await
    .map_err(Into::into)
}

fn has_blob_storage_capacity(
    stored_bytes: i64,
    pending_bytes: i64,
    requested_bytes: i64,
    storage_limit: i64,
) -> bool {
    stored_bytes >= 0
        && pending_bytes >= 0
        && requested_bytes >= 0
        && storage_limit > 0
        && stored_bytes
            .checked_add(pending_bytes)
            .and_then(|reserved| reserved.checked_add(requested_bytes))
            .is_some_and(|total| total <= storage_limit)
}

fn has_upload_capacity(
    pending_uploads: i64,
    pending_bytes: i64,
    stored_bytes: i64,
    requested_bytes: i64,
    storage_limit: i64,
) -> bool {
    pending_uploads >= 0
        && pending_uploads < MAX_PENDING_BLOB_UPLOADS
        && pending_bytes >= 0
        && pending_bytes
            .checked_add(requested_bytes)
            .is_some_and(|total| total <= MAX_PENDING_BLOB_UPLOAD_BYTES)
        && has_blob_storage_capacity(stored_bytes, pending_bytes, requested_bytes, storage_limit)
}

async fn blob_upload_status(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, project_id, upload_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<BlobUploadStatus>, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    let upload = upload_metadata(&state, team_id, project_id, upload_id, user.id).await?;
    let received = sqlx::query_scalar::<_, i32>(
        "SELECT chunk_index FROM blob_upload_chunks WHERE upload_id=$1 ORDER BY chunk_index",
    )
    .bind(upload_id)
    .fetch_all(&state.database)
    .await?;
    let uploaded_chunks = received
        .into_iter()
        .map(|index| u32::try_from(index).map_err(|_| ApiError::internal()))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(BlobUploadStatus {
        upload_id,
        content_hash: upload.content_hash.trim().to_owned(),
        total_bytes: upload.total_bytes,
        uploaded_chunks,
        total_chunks: chunk_count(upload.total_bytes)?,
        complete: upload.complete,
    }))
}

async fn upload_blob_chunk(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, project_id, upload_id, chunk_index)): Path<(Uuid, Uuid, Uuid, u32)>,
    body: Body,
) -> Result<StatusCode, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if !role.can_write() {
        return Err(ApiError::forbidden());
    }
    ensure_project_exists(&state, team_id, project_id).await?;
    let mut transaction = state.database.begin().await?;
    let upload =
        upload_metadata_tx(&mut transaction, team_id, project_id, upload_id, user.id).await?;
    if upload.complete {
        return Err(ApiError::conflict("blob_upload_already_completed"));
    }
    let expected_size = expected_chunk_size(upload.total_bytes, chunk_index)
        .ok_or_else(|| ApiError::bad_request("chunk_index_out_of_range"))?;

    let upload_dir = state
        .storage_dir
        .join("staging")
        .join("uploads")
        .join(upload_id.to_string());
    fs::create_dir_all(&upload_dir)
        .await
        .map_err(|_| ApiError::internal())?;
    let chunk_path = upload_dir.join(format!("{chunk_index:08}.part"));
    let staged_path = upload_dir.join(format!("{}.tmp", Uuid::now_v7()));
    let staged = StagedBlob(staged_path.clone());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_path)
        .await
        .map_err(|_| ApiError::internal())?;
    let mut byte_size = 0_u64;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::bad_request("invalid_blob_chunk"))?;
        byte_size = byte_size
            .checked_add(chunk.len() as u64)
            .filter(|size| *size <= expected_size)
            .ok_or_else(|| ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "chunk_too_large"))?;
        file.write_all(&chunk)
            .await
            .map_err(|_| ApiError::internal())?;
    }
    if byte_size != expected_size {
        return Err(ApiError::bad_request("chunk_size_mismatch"));
    }
    file.sync_all().await.map_err(|_| ApiError::internal())?;
    drop(file);
    fs::rename(&staged_path, &chunk_path)
        .await
        .map_err(|_| ApiError::internal())?;
    drop(staged);

    sqlx::query("INSERT INTO blob_upload_chunks (upload_id,chunk_index,byte_size) VALUES ($1,$2,$3) ON CONFLICT (upload_id,chunk_index) DO UPDATE SET byte_size=EXCLUDED.byte_size,created_at=now()")
        .bind(upload_id)
        .bind(i32::try_from(chunk_index).map_err(|_| ApiError::bad_request("chunk_index_out_of_range"))?)
        .bind(i64::try_from(byte_size).map_err(|_| ApiError::bad_request("chunk_too_large"))?)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn complete_blob_upload(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, project_id, upload_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<BlobReceipt>, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if !role.can_write() {
        return Err(ApiError::forbidden());
    }
    ensure_project_exists(&state, team_id, project_id).await?;
    let mut transaction = state.database.begin().await?;
    let upload =
        upload_metadata_tx(&mut transaction, team_id, project_id, upload_id, user.id).await?;
    if upload.complete {
        transaction.commit().await?;
        return Ok(Json(BlobReceipt {
            content_hash: upload.content_hash.trim().to_owned(),
            byte_size: upload.total_bytes,
        }));
    }

    let expected_chunks = chunk_count(upload.total_bytes)?;
    let chunks = sqlx::query_as::<_, (i32, i64)>(
        "SELECT chunk_index,byte_size FROM blob_upload_chunks WHERE upload_id=$1 ORDER BY chunk_index",
    )
    .bind(upload_id)
    .fetch_all(&mut *transaction)
    .await?;
    if chunks.len() != expected_chunks as usize
        || chunks
            .iter()
            .enumerate()
            .any(|(index, (chunk_index, size))| {
                i32::try_from(index).ok() != Some(*chunk_index)
                    || u64::try_from(*size).ok()
                        != expected_chunk_size(upload.total_bytes, index as u32)
            })
    {
        return Err(ApiError::conflict("blob_upload_incomplete"));
    }

    let upload_dir = state
        .storage_dir
        .join("staging")
        .join("uploads")
        .join(upload_id.to_string());
    let staged_path = state
        .storage_dir
        .join("staging")
        .join(format!("{}.complete", Uuid::now_v7()));
    let staged = StagedBlob(staged_path.clone());
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_path)
        .await
        .map_err(|_| ApiError::internal())?;
    let mut hasher = Sha256::new();
    let mut total_bytes = 0_u64;
    let mut buffer = vec![0_u8; BLOB_CHUNK_BYTES];
    for (index, _) in &chunks {
        let chunk_path = upload_dir.join(format!("{index:08}.part"));
        let mut chunk = File::open(chunk_path)
            .await
            .map_err(|_| ApiError::conflict("blob_chunk_missing"))?;
        loop {
            let read = chunk
                .read(&mut buffer)
                .await
                .map_err(|_| ApiError::internal())?;
            if read == 0 {
                break;
            }
            total_bytes = total_bytes
                .checked_add(read as u64)
                .ok_or_else(|| ApiError::bad_request("blob_too_large"))?;
            hasher.update(&buffer[..read]);
            output
                .write_all(&buffer[..read])
                .await
                .map_err(|_| ApiError::internal())?;
        }
    }
    if total_bytes != upload.total_bytes
        || format!("{:x}", hasher.finalize()) != upload.content_hash.trim()
    {
        return Err(ApiError::bad_request("blob_hash_mismatch"));
    }
    output.sync_all().await.map_err(|_| ApiError::internal())?;
    drop(output);

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BLOB_UPLOAD_ADVISORY_LOCK)
        .execute(&mut *transaction)
        .await?;

    let blob_dir = state
        .storage_dir
        .join("blobs")
        .join(&upload.content_hash[..2]);
    fs::create_dir_all(&blob_dir)
        .await
        .map_err(|_| ApiError::internal())?;
    let blob_path = blob_dir.join(upload.content_hash.trim());
    fs::rename(&staged_path, &blob_path)
        .await
        .map_err(|_| ApiError::internal())?;
    drop(staged);

    sqlx::query("INSERT INTO project_blobs (team_id,project_id,content_hash,byte_size,uploaded_by) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (team_id,project_id,content_hash) DO UPDATE SET byte_size=EXCLUDED.byte_size,uploaded_by=EXCLUDED.uploaded_by")
        .bind(team_id)
        .bind(project_id)
        .bind(upload.content_hash.trim())
        .bind(i64::try_from(upload.total_bytes).map_err(|_| ApiError::bad_request("blob_too_large"))?)
        .bind(user.id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE blob_uploads SET completed_at=now() WHERE id=$1")
        .bind(upload_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM blob_upload_chunks WHERE upload_id=$1")
        .bind(upload_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    let _ = fs::remove_dir_all(upload_dir).await;

    tracing::info!(event = "project_blob_uploaded", team_id = %team_id, project_id = %project_id, actor_id = %user.id, byte_size = upload.total_bytes, transfer = "chunked", "Project blob upload completed");
    Ok(Json(BlobReceipt {
        content_hash: upload.content_hash.trim().to_owned(),
        byte_size: upload.total_bytes,
    }))
}

async fn cancel_blob_upload(
    State(state): State<AppState>,
    axum::extract::Extension(user): axum::extract::Extension<AuthUser>,
    Path((team_id, project_id, upload_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    let result = sqlx::query(
        "DELETE FROM blob_uploads WHERE id=$1 AND team_id=$2 AND project_id=$3 AND created_by=$4",
    )
    .bind(upload_id)
    .bind(team_id)
    .bind(project_id)
    .bind(user.id)
    .execute(&state.database)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found("blob_upload_not_found"));
    }
    let upload_dir = state
        .storage_dir
        .join("staging")
        .join("uploads")
        .join(upload_id.to_string());
    let _ = fs::remove_dir_all(upload_dir).await;
    Ok(StatusCode::NO_CONTENT)
}

struct UploadMetadata {
    content_hash: String,
    total_bytes: u64,
    complete: bool,
}

async fn upload_metadata(
    state: &AppState,
    team_id: Uuid,
    project_id: Uuid,
    upload_id: Uuid,
    user_id: Uuid,
) -> Result<UploadMetadata, ApiError> {
    let row = sqlx::query_as::<_, (String, i64, bool)>(
        "SELECT content_hash,total_bytes,completed_at IS NOT NULL FROM blob_uploads WHERE id=$1 AND team_id=$2 AND project_id=$3 AND created_by=$4 AND expires_at>now()",
    )
    .bind(upload_id)
    .bind(team_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(&state.database)
    .await?
    .ok_or_else(|| ApiError::not_found("blob_upload_not_found"))?;
    parse_upload_metadata(row)
}

async fn upload_metadata_tx(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    team_id: Uuid,
    project_id: Uuid,
    upload_id: Uuid,
    user_id: Uuid,
) -> Result<UploadMetadata, ApiError> {
    let row = sqlx::query_as::<_, (String, i64, bool)>(
        "SELECT content_hash,total_bytes,completed_at IS NOT NULL FROM blob_uploads WHERE id=$1 AND team_id=$2 AND project_id=$3 AND created_by=$4 AND expires_at>now() FOR UPDATE",
    )
    .bind(upload_id)
    .bind(team_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("blob_upload_not_found"))?;
    parse_upload_metadata(row)
}

fn parse_upload_metadata(row: (String, i64, bool)) -> Result<UploadMetadata, ApiError> {
    Ok(UploadMetadata {
        content_hash: row.0.trim().to_owned(),
        total_bytes: u64::try_from(row.1).map_err(|_| ApiError::internal())?,
        complete: row.2,
    })
}

fn chunk_count(total_bytes: u64) -> Result<u32, ApiError> {
    let chunk_size = BLOB_CHUNK_BYTES as u64;
    let count = total_bytes.div_ceil(chunk_size);
    u32::try_from(count).map_err(|_| ApiError::bad_request("blob_too_large"))
}

fn expected_chunk_size(total_bytes: u64, chunk_index: u32) -> Option<u64> {
    let chunk_size = BLOB_CHUNK_BYTES as u64;
    let count = chunk_count(total_bytes).ok()?;
    if chunk_index >= count {
        return None;
    }
    let offset = u64::from(chunk_index).checked_mul(chunk_size)?;
    Some((total_bytes - offset).min(chunk_size))
}

fn validate_hash(value: &str) -> Result<(), ApiError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ApiError::bad_request("invalid_content_hash"))
    }
}

struct StagedBlob(std::path::PathBuf);

impl Drop for StagedBlob {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod upload_capacity_tests {
    use super::*;

    #[test]
    fn pending_upload_limit_bounds_bytes_and_zero_byte_sessions() {
        let storage_limit = 8 * 1024 * 1024 * 1024;
        assert!(has_upload_capacity(
            0,
            0,
            0,
            MAX_BLOB_BYTES as i64,
            storage_limit
        ));
        assert!(has_upload_capacity(
            3,
            3 * MAX_BLOB_BYTES as i64,
            0,
            MAX_BLOB_BYTES as i64,
            storage_limit
        ));
        assert!(!has_upload_capacity(
            4,
            4 * MAX_BLOB_BYTES as i64,
            0,
            1,
            storage_limit
        ));
        assert!(!has_upload_capacity(8, 0, 0, 0, storage_limit));
        assert!(!has_upload_capacity(
            1,
            MAX_PENDING_BLOB_UPLOAD_BYTES,
            0,
            1,
            storage_limit
        ));
        assert!(!has_upload_capacity(-1, 0, 0, 0, storage_limit));
        assert!(!has_upload_capacity(0, 0, 0, -1, storage_limit));
        assert!(!has_upload_capacity(0, 0, storage_limit, 1, storage_limit));
    }
}
