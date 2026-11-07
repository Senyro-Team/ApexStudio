use std::sync::Arc;

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    body::Body,
    extract::{Extension, Path, Request, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::AppState;

const SESSION_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;
const INVITATION_TTL_HOURS: i64 = 72;

#[derive(Clone)]
pub struct AuthUser {
    pub id: Uuid,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: &'static str,
}

pub(crate) struct ApiError {
    status: StatusCode,
    code: &'static str,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, code: &'static str) -> Self {
        Self { status, code }
    }

    pub(crate) fn bad_request(code: &'static str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code)
    }

    pub(crate) fn unauthorized(code: &'static str) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code)
    }

    pub(crate) fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "insufficient_team_role")
    }

    pub(crate) fn not_found(code: &'static str) -> Self {
        Self::new(StatusCode::NOT_FOUND, code)
    }

    pub(crate) fn conflict(code: &'static str) -> Self {
        Self::new(StatusCode::CONFLICT, code)
    }

    pub(crate) fn internal() -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(
                event = "api_error",
                status = self.status.as_u16(),
                code = self.code,
                "Sync API operation failed"
            );
        } else {
            tracing::warn!(
                event = "api_error",
                status = self.status.as_u16(),
                code = self.code,
                "Sync API request rejected"
            );
        }
        (self.status, Json(ErrorResponse { error: self.code })).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(database_error) = &error {
            if database_error.code().as_deref() == Some("23505") {
                return Self::conflict("already_exists");
            }
        }
        tracing::error!("Authentication or team database operation failed.");
        Self::internal()
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TeamRole {
    Owner,
    Admin,
    Designer,
    ReadOnly,
}

impl TeamRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Designer => "designer",
            Self::ReadOnly => "read_only",
        }
    }

    fn from_str(value: &str) -> Result<Self, ApiError> {
        match value {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "designer" => Ok(Self::Designer),
            "read_only" => Ok(Self::ReadOnly),
            _ => Err(ApiError::internal()),
        }
    }

    pub(crate) fn can_write(self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Designer)
    }

    fn can_manage_members(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapRequest {
    bootstrap_token: String,
    team_name: String,
    email: String,
    display_name: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptInvitationRequest {
    invitation_token: String,
    email: String,
    display_name: String,
    password: String,
}

#[derive(Deserialize)]
struct CreateInvitationRequest {
    role: TeamRole,
}

#[derive(Deserialize)]
struct ChangeRoleRequest {
    role: TeamRole,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    access_token: String,
    token_type: &'static str,
    expires_in_seconds: u64,
    user_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TeamSummary {
    id: Uuid,
    name: String,
    role: TeamRole,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentUser {
    id: Uuid,
    email: String,
    display_name: String,
    teams: Vec<TeamSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InvitationResponse {
    id: Uuid,
    invitation_token: String,
    role: TeamRole,
    expires_in_seconds: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TeamMember {
    id: Uuid,
    email: String,
    display_name: String,
    role: TeamRole,
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/setup", post(bootstrap))
        .route("/api/v1/sessions", post(login))
        .route("/api/v1/invitations/accept", post(accept_invitation))
}

pub fn private_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/session", get(current_user))
        .route("/api/v1/sessions/current", delete(logout))
        .route("/api/v1/teams/{team_id}/members", get(list_members))
        .route(
            "/api/v1/teams/{team_id}/members/{user_id}",
            delete(remove_member).patch(change_role),
        )
        .route(
            "/api/v1/teams/{team_id}/invitations",
            post(create_invitation),
        )
        .route_layer(middleware::from_fn_with_state(state, authenticate))
}

pub fn protect(routes: Router<AppState>, state: AppState) -> Router<AppState> {
    routes.route_layer(middleware::from_fn_with_state(state, authenticate))
}

async fn authenticate(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let Some(token) = bearer_token(request.headers()) else {
        return ApiError::unauthorized("authentication_required").into_response();
    };
    let token_hash = hash_token(token);
    let user_id = match sqlx::query_scalar::<_, Uuid>(
        "SELECT s.user_id FROM auth_sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.revoked_at IS NULL AND s.expires_at>now() AND u.disabled_at IS NULL",
    )
    .bind(token_hash)
    .fetch_optional(&state.database)
    .await
    {
        Ok(Some(user_id)) => user_id,
        Ok(None) => return ApiError::unauthorized("invalid_session").into_response(),
        Err(_) => return ApiError::internal().into_response(),
    };

    request.extensions_mut().insert(AuthUser { id: user_id });
    next.run(request).await
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty())
}

async fn bootstrap(
    State(state): State<AppState>,
    Json(mut input): Json<BootstrapRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    let configured_hash = state
        .bootstrap_token_hash
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "setup_unavailable"))?;
    let supplied_hash: [u8; 32] = Sha256::digest(input.bootstrap_token.as_bytes()).into();
    input.bootstrap_token.zeroize();
    if !bool::from(configured_hash.ct_eq(&supplied_hash)) {
        return Err(ApiError::unauthorized("invalid_setup_token"));
    }

    let email = normalize_email(&input.email)?;
    let team_name = validate_name(&input.team_name, 2, 80, "invalid_team_name")?;
    let display_name = validate_name(&input.display_name, 1, 80, "invalid_display_name")?;
    validate_password(&input.password)?;
    let password_hash = hash_password(&state, input.password).await?;

    let user_id = Uuid::now_v7();
    let team_id = Uuid::now_v7();
    let session = new_session(user_id)?;
    let mut transaction = state.database.begin().await?;
    let initialized =
        sqlx::query_scalar::<_, bool>("SELECT initialized FROM server_setup WHERE id=1 FOR UPDATE")
            .fetch_one(&mut *transaction)
            .await?;
    if initialized {
        return Err(ApiError::conflict("server_already_initialized"));
    }

    sqlx::query("INSERT INTO users (id,email,display_name,password_hash) VALUES ($1,$2,$3,$4)")
        .bind(user_id)
        .bind(email)
        .bind(display_name)
        .bind(password_hash)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO teams (id,name,owner_user_id) VALUES ($1,$2,$3)")
        .bind(team_id)
        .bind(team_name)
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO team_memberships (team_id,user_id,role) VALUES ($1,$2,'owner')")
        .bind(team_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE server_setup SET initialized=TRUE WHERE id=1")
        .execute(&mut *transaction)
        .await?;
    insert_session(&mut transaction, &session).await?;
    transaction.commit().await?;

    Ok(Json(session.response()))
}

async fn login(
    State(state): State<AppState>,
    Json(mut input): Json<LoginRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    let email = match normalize_email(&input.email) {
        Ok(email) => email,
        Err(error) => {
            input.password.zeroize();
            return Err(error);
        }
    };
    if input.password.len() > 128 {
        input.password.zeroize();
        return Err(ApiError::unauthorized("invalid_credentials"));
    }
    let row = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT id,password_hash,disabled_at IS NULL FROM users WHERE email=$1",
    )
    .bind(email)
    .fetch_optional(&state.database)
    .await?;

    let (user_id, encoded_hash, enabled) = match row {
        Some(row) => row,
        None => {
            let password = std::mem::take(&mut input.password);
            let _ = hash_password(&state, password).await?;
            return Err(ApiError::unauthorized("invalid_credentials"));
        }
    };

    let password = std::mem::take(&mut input.password);
    if !verify_password(&state, password, encoded_hash).await? || !enabled {
        return Err(ApiError::unauthorized("invalid_credentials"));
    }

    let session = new_session(user_id)?;
    let mut transaction = state.database.begin().await?;
    insert_session(&mut transaction, &session).await?;
    transaction.commit().await?;
    Ok(Json(session.response()))
}

async fn accept_invitation(
    State(state): State<AppState>,
    Json(mut input): Json<AcceptInvitationRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    let email = match normalize_email(&input.email) {
        Ok(email) => email,
        Err(error) => {
            input.password.zeroize();
            return Err(error);
        }
    };
    let display_name = validate_name(&input.display_name, 1, 80, "invalid_display_name")?;
    validate_password(&input.password)?;
    let password_hash = hash_password(&state, input.password).await?;
    let invitation_hash = hash_token(&input.invitation_token);
    input.invitation_token.zeroize();

    let user_id = Uuid::now_v7();
    let session = new_session(user_id)?;
    let mut transaction = state.database.begin().await?;
    let invitation = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT team_id,role FROM team_invitations WHERE token_hash=$1 AND used_at IS NULL AND expires_at>now() FOR UPDATE",
    )
    .bind(&invitation_hash)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("invitation_unavailable"))?;

    sqlx::query("INSERT INTO users (id,email,display_name,password_hash) VALUES ($1,$2,$3,$4)")
        .bind(user_id)
        .bind(email)
        .bind(display_name)
        .bind(password_hash)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO team_memberships (team_id,user_id,role) VALUES ($1,$2,$3)")
        .bind(invitation.0)
        .bind(user_id)
        .bind(&invitation.1)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE team_invitations SET used_at=now() WHERE token_hash=$1")
        .bind(invitation_hash)
        .execute(&mut *transaction)
        .await?;
    insert_session(&mut transaction, &session).await?;
    transaction.commit().await?;

    Ok(Json(session.response()))
}

async fn current_user(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<CurrentUser>, ApiError> {
    let (email, display_name) = sqlx::query_as::<_, (String, String)>(
        "SELECT email,display_name FROM users WHERE id=$1 AND disabled_at IS NULL",
    )
    .bind(user.id)
    .fetch_optional(&state.database)
    .await?
    .ok_or_else(|| ApiError::unauthorized("invalid_session"))?;
    let rows = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT t.id,t.name,m.role FROM team_memberships m JOIN teams t ON t.id=m.team_id WHERE m.user_id=$1 ORDER BY t.name",
    )
    .bind(user.id)
    .fetch_all(&state.database)
    .await?;

    let teams = rows
        .into_iter()
        .map(|(id, name, role)| {
            Ok(TeamSummary {
                id,
                name,
                role: TeamRole::from_str(&role)?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;

    Ok(Json(CurrentUser {
        id: user.id,
        email,
        display_name,
        teams,
    }))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    let token = bearer_token(&headers).ok_or_else(|| ApiError::unauthorized("invalid_session"))?;
    sqlx::query(
        "UPDATE auth_sessions SET revoked_at=now() WHERE token_hash=$1 AND revoked_at IS NULL",
    )
    .bind(hash_token(token))
    .execute(&state.database)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_invitation(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(team_id): Path<Uuid>,
    Json(input): Json<CreateInvitationRequest>,
) -> Result<Json<InvitationResponse>, ApiError> {
    let actor_role = membership_role(&state, team_id, user.id).await?;
    if !actor_role.can_manage_members()
        || (actor_role == TeamRole::Admin && input.role == TeamRole::Admin)
        || input.role == TeamRole::Owner
    {
        return Err(ApiError::forbidden());
    }

    let (token, token_hash) = new_token()?;
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO team_invitations (id,team_id,token_hash,role,created_by,expires_at) VALUES ($1,$2,$3,$4,$5,now()+interval '72 hours')")
        .bind(id)
        .bind(team_id)
        .bind(token_hash)
        .bind(input.role.as_str())
        .bind(user.id)
        .execute(&state.database)
        .await?;

    Ok(Json(InvitationResponse {
        id,
        invitation_token: token,
        role: input.role,
        expires_in_seconds: (INVITATION_TTL_HOURS as u64) * 60 * 60,
    }))
}

async fn list_members(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(team_id): Path<Uuid>,
) -> Result<Json<Vec<TeamMember>>, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    let rows = sqlx::query_as::<_, (Uuid, String, String, String)>(
        "SELECT u.id,u.email,u.display_name,m.role FROM team_memberships m JOIN users u ON u.id=m.user_id WHERE m.team_id=$1 AND u.disabled_at IS NULL ORDER BY m.joined_at,u.email",
    )
    .bind(team_id)
    .fetch_all(&state.database)
    .await?;

    let members = rows
        .into_iter()
        .map(|(id, email, display_name, role)| {
            Ok(TeamMember {
                id,
                email,
                display_name,
                role: TeamRole::from_str(&role)?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(members))
}

async fn remove_member(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((team_id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let mut transaction = state.database.begin().await?;
    lock_team_membership_changes(&mut transaction, team_id).await?;
    let actor_role = member_role_in_transaction(&mut transaction, team_id, user.id).await?;
    let target_role = member_role_in_transaction(&mut transaction, team_id, target_id).await?;
    if !actor_role.can_manage_members()
        || target_role == TeamRole::Owner
        || (actor_role == TeamRole::Admin && target_role == TeamRole::Admin)
    {
        return Err(ApiError::forbidden());
    }
    sqlx::query("DELETE FROM team_memberships WHERE team_id=$1 AND user_id=$2")
        .bind(team_id)
        .bind(target_id)
        .execute(&mut *transaction)
        .await?;
    // Revoke every active session so a kicked member is signed out immediately.
    sqlx::query(
        "UPDATE auth_sessions SET revoked_at=now() WHERE user_id=$1 AND revoked_at IS NULL",
    )
    .bind(target_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    tracing::info!(event = "team_member_removed", team_id = %team_id, actor_id = %user.id, target_user_id = %target_id, "Team member removed and active sessions revoked");
    Ok(StatusCode::NO_CONTENT)
}

async fn change_role(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((team_id, target_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<ChangeRoleRequest>,
) -> Result<StatusCode, ApiError> {
    let mut transaction = state.database.begin().await?;
    lock_team_membership_changes(&mut transaction, team_id).await?;
    let actor_role = member_role_in_transaction(&mut transaction, team_id, user.id).await?;
    let target_role = member_role_in_transaction(&mut transaction, team_id, target_id).await?;
    if target_role == TeamRole::Owner
        || input.role == TeamRole::Owner
        || (actor_role != TeamRole::Owner
            && (!actor_role.can_manage_members()
                || target_role == TeamRole::Admin
                || input.role == TeamRole::Admin))
    {
        return Err(ApiError::forbidden());
    }
    sqlx::query("UPDATE team_memberships SET role=$1 WHERE team_id=$2 AND user_id=$3")
        .bind(input.role.as_str())
        .bind(team_id)
        .bind(target_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn lock_team_membership_changes(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("team-members:{team_id}"))
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn member_role_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<TeamRole, ApiError> {
    let role = sqlx::query_scalar::<_, String>(
        "SELECT role FROM team_memberships WHERE team_id=$1 AND user_id=$2",
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("team_membership_not_found"))?;
    TeamRole::from_str(&role)
}

pub(crate) async fn membership_role(
    state: &AppState,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<TeamRole, ApiError> {
    let role = sqlx::query_scalar::<_, String>(
        "SELECT role FROM team_memberships WHERE team_id=$1 AND user_id=$2",
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_optional(&state.database)
    .await?
    .ok_or_else(ApiError::forbidden)?;
    TeamRole::from_str(&role)
}

fn validate_password(password: &str) -> Result<(), ApiError> {
    if !(12..=128).contains(&password.chars().count()) {
        return Err(ApiError::bad_request("password_length_invalid"));
    }
    Ok(())
}

fn normalize_email(email: &str) -> Result<String, ApiError> {
    let normalized = email.trim().to_lowercase();
    let Some((local, domain)) = normalized.split_once('@') else {
        return Err(ApiError::bad_request("invalid_email"));
    };
    if normalized.len() > 254
        || local.is_empty()
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
        || normalized.chars().any(char::is_whitespace)
    {
        return Err(ApiError::bad_request("invalid_email"));
    }
    Ok(normalized)
}

fn validate_name(
    value: &str,
    min: usize,
    max: usize,
    error: &'static str,
) -> Result<String, ApiError> {
    let value = value.trim();
    if !(min..=max).contains(&value.chars().count()) || value.chars().any(char::is_control) {
        return Err(ApiError::bad_request(error));
    }
    Ok(value.to_owned())
}

async fn hash_password(state: &AppState, mut password: String) -> Result<String, ApiError> {
    if let Err(error) = validate_password(&password) {
        password.zeroize();
        return Err(error);
    }
    let permit = Arc::clone(&state.password_hash_slots)
        .acquire_owned()
        .await
        .map_err(|_| ApiError::internal())?;
    tokio::task::spawn_blocking(move || {
        let result = {
            let salt = SaltString::generate(&mut OsRng);
            Argon2::default()
                .hash_password(password.as_bytes(), &salt)
                .map(|hash| hash.to_string())
                .map_err(|_| ApiError::internal())
        };
        password.zeroize();
        drop(permit);
        result
    })
    .await
    .map_err(|_| ApiError::internal())?
}

async fn verify_password(
    state: &AppState,
    mut password: String,
    encoded_hash: String,
) -> Result<bool, ApiError> {
    let permit = Arc::clone(&state.password_hash_slots)
        .acquire_owned()
        .await
        .map_err(|_| ApiError::internal())?;
    tokio::task::spawn_blocking(move || {
        let result = match PasswordHash::new(&encoded_hash) {
            Ok(parsed_hash) => Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok()),
            Err(_) => Err(ApiError::internal()),
        };
        password.zeroize();
        drop(permit);
        result
    })
    .await
    .map_err(|_| ApiError::internal())?
}

struct NewSession {
    id: Uuid,
    user_id: Uuid,
    token: String,
    token_hash: Vec<u8>,
}

impl NewSession {
    fn response(self) -> SessionResponse {
        SessionResponse {
            access_token: self.token,
            token_type: "Bearer",
            expires_in_seconds: SESSION_TTL_SECONDS,
            user_id: self.user_id,
        }
    }
}

fn new_session(user_id: Uuid) -> Result<NewSession, ApiError> {
    let (token, token_hash) = new_token()?;
    Ok(NewSession {
        id: Uuid::now_v7(),
        user_id,
        token,
        token_hash,
    })
}

async fn insert_session(
    transaction: &mut Transaction<'_, Postgres>,
    session: &NewSession,
) -> Result<(), ApiError> {
    if session.token.is_empty() || session.token_hash.is_empty() {
        return Err(ApiError::internal());
    }
    sqlx::query("INSERT INTO auth_sessions (id,user_id,token_hash,expires_at) VALUES ($1,$2,$3,now()+interval '30 days')")
        .bind(session.id)
        .bind(session.user_id)
        .bind(&session.token_hash)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

fn new_token() -> Result<(String, Vec<u8>), ApiError> {
    let mut bytes = [0_u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| ApiError::internal())?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    Ok((token.clone(), hash_token(&token)))
}

fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
