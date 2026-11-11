use axum::{
    body::{to_bytes, Body},
    extract::{DefaultBodyLimit, Extension, Path, Query, Request, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::{get, patch},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{net::IpAddr, time::Duration};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    auth::{membership_role, ApiError, AuthUser},
    AppState,
};

const MAX_IMAGE_UPLOAD_BYTES: usize = 50 * 1024 * 1024;
const MAX_PROVIDER_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct ServiceEndpoint {
    origin: Url,
    key: Zeroizing<String>,
}

#[derive(Clone, Default)]
pub struct ServiceConfig {
    image_provider: Option<ServiceEndpoint>,
    invite_tracker: Option<ServiceEndpoint>,
}

impl ServiceConfig {
    pub fn from_env() -> Result<Self, std::io::Error> {
        Ok(Self {
            image_provider: endpoint_from_env(
                "APEX_IMAGE_PROVIDER_URL",
                "APEX_IMAGE_PROVIDER_KEY",
            )?,
            invite_tracker: endpoint_from_env(
                "APEX_INVITE_TRACKER_URL",
                "APEX_INVITE_TRACKER_KEY",
            )?,
        })
    }
}

fn endpoint_from_env(
    url_name: &str,
    key_name: &str,
) -> Result<Option<ServiceEndpoint>, std::io::Error> {
    let origin = std::env::var(url_name)
        .ok()
        .filter(|v| !v.trim().is_empty());
    let key = std::env::var(key_name)
        .ok()
        .filter(|v| !v.trim().is_empty());
    match (origin, key) {
        (None, None) => Ok(None),
        (Some(origin), Some(key)) => {
            let parsed = Url::parse(origin.trim()).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{url_name} must be a valid HTTP(S) origin"),
                )
            })?;
            if !["https", "http"].contains(&parsed.scheme())
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.query().is_some()
                || parsed.fragment().is_some()
                || parsed.path() != "/"
                || key.trim().len() < 32
                || !has_safe_service_transport(&parsed)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Configure a valid service URL and a key of at least 32 characters for {url_name}; public service URLs must use HTTPS"),
                ));
            }
            Ok(Some(ServiceEndpoint {
                origin: parsed,
                key: Zeroizing::new(key),
            }))
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Set both {url_name} and {key_name}, or leave both unset"),
        )),
    }
}

fn is_private_service_origin(origin: &Url) -> bool {
    let Some(host) = origin.host_str() else {
        return false;
    };
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(address) = host.parse::<IpAddr>() {
        return match address {
            IpAddr::V4(address) => {
                address.is_loopback() || address.is_private() || address.is_link_local()
            }
            IpAddr::V6(address) => {
                address.is_loopback()
                    || address.is_unique_local()
                    || address.is_unicast_link_local()
            }
        };
    }

    let host = host.trim_end_matches('.');
    !host.contains('.')
        || ["localhost", "local", "internal", "lan", "test"]
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

fn has_safe_service_transport(origin: &Url) -> bool {
    origin.scheme() == "https" || (origin.scheme() == "http" && is_private_service_origin(origin))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceAvailability {
    project_sync: bool,
    image_hosting: bool,
    invite_tracking: bool,
    image_hosting_reachable: Option<bool>,
    invite_tracking_reachable: Option<bool>,
}

#[derive(Deserialize)]
struct ShareRequest {
    scope: String,
}

pub fn private_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/teams/{team_id}/services",
            get(service_availability),
        )
        .route(
            "/api/v1/teams/{team_id}/image-library",
            get(list_images).post(upload_images),
        )
        .route(
            "/api/v1/teams/{team_id}/image-library/{image_id}/sharing",
            patch(set_image_scope),
        )
        .route(
            "/api/v1/teams/{team_id}/invite-campaigns",
            get(list_invite_campaigns).post(create_invite_campaign),
        )
        .layer(DefaultBodyLimit::max(MAX_IMAGE_UPLOAD_BYTES))
}

async fn service_availability(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(team_id): Path<Uuid>,
) -> Result<Json<ServiceAvailability>, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    let (image_hosting_reachable, invite_tracking_reachable) = tokio::join!(
        endpoint_health(state.services.image_provider.as_ref(), "health"),
        endpoint_health(state.services.invite_tracker.as_ref(), "api/health"),
    );
    Ok(Json(ServiceAvailability {
        project_sync: true,
        image_hosting: state.services.image_provider.is_some(),
        invite_tracking: state.services.invite_tracker.is_some(),
        image_hosting_reachable,
        invite_tracking_reachable,
    }))
}

async fn endpoint_health(service: Option<&ServiceEndpoint>, path: &str) -> Option<bool> {
    let service = service?;
    let url = service_url(service, path).ok()?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(3))
        .build()
        .ok()?;
    Some(
        client
            .get(url)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success()),
    )
}

async fn list_invite_campaigns(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(team_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    let service = invite_tracker(&state)?;
    proxy_json(
        provider_client()?
            .get(service_url(service, "api/service/campaigns")?)
            .header("x-apex-service-key", service.key.as_str())
            .header("x-apex-user-id", user.id.to_string())
            .header("x-apex-team-id", team_id.to_string())
            .send()
            .await,
    )
    .await
}

async fn create_invite_campaign(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(team_id): Path<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if !role.can_write() {
        return Err(ApiError::forbidden());
    }
    let request_id = request
        .headers()
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| ApiError::bad_request("valid_idempotency_key_required"))?;
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.to_ascii_lowercase().starts_with("application/json"))
        .map(str::to_owned)
        .ok_or_else(|| ApiError::bad_request("json_body_required"))?;
    let body = to_bytes(request.into_body(), 64 * 1024)
        .await
        .map_err(|_| ApiError::bad_request("campaign_request_too_large"))?;
    let service = invite_tracker(&state)?;
    proxy_json(
        provider_client()?
            .post(service_url(service, "api/service/campaigns")?)
            .header("content-type", content_type)
            .header("x-apex-service-key", service.key.as_str())
            .header("x-apex-user-id", user.id.to_string())
            .header("x-apex-team-id", team_id.to_string())
            .header("idempotency-key", request_id.to_string())
            .body(body)
            .send()
            .await,
    )
    .await
}

#[derive(Deserialize)]
struct ImageQuery {
    #[serde(default = "default_scope")]
    scope: String,
}

fn default_scope() -> String {
    "available".to_owned()
}

async fn list_images(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(team_id): Path<Uuid>,
    Query(query): Query<ImageQuery>,
) -> Result<Response, ApiError> {
    membership_role(&state, team_id, user.id).await?;
    let service = image_provider(&state)?;
    if !["available", "personal", "team"].contains(&query.scope.as_str()) {
        return Err(ApiError::bad_request("invalid_image_scope"));
    }
    let mut url = service_url(service, "api/service/images")?;
    url.query_pairs_mut().append_pair("scope", &query.scope);
    proxy_json(
        provider_client()?
            .get(url)
            .header("x-apex-service-key", service.key.as_str())
            .header("x-apex-user-id", user.id.to_string())
            .header("x-apex-team-id", team_id.to_string())
            .send()
            .await,
    )
    .await
}

async fn upload_images(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(team_id): Path<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if !role.can_write() {
        return Err(ApiError::forbidden());
    }
    let service = image_provider(&state)?;
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            value
                .to_ascii_lowercase()
                .starts_with("multipart/form-data;")
        })
        .map(str::to_owned)
        .ok_or_else(|| ApiError::bad_request("multipart_upload_required"))?;
    let scope = request.uri().query().unwrap_or_default();
    let scope = url::form_urlencoded::parse(scope.as_bytes())
        .find(|(key, _)| key == "scope")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_else(|| "personal".to_owned());
    if !["personal", "team"].contains(&scope.as_str()) {
        return Err(ApiError::bad_request("invalid_image_scope"));
    }
    let body = to_bytes(request.into_body(), MAX_IMAGE_UPLOAD_BYTES)
        .await
        .map_err(|_| ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "image_upload_too_large"))?;
    let url = service_url(service, "api/service/images")?;
    let response = provider_client()?
        .post(url)
        .header("content-type", content_type)
        .header("x-apex-service-key", service.key.as_str())
        .header("x-apex-user-id", user.id.to_string())
        .header("x-apex-team-id", team_id.to_string())
        .header("x-apex-scope", scope)
        .body(body)
        .send()
        .await;
    proxy_json(response).await
}

async fn set_image_scope(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((team_id, image_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<ShareRequest>,
) -> Result<Response, ApiError> {
    let role = membership_role(&state, team_id, user.id).await?;
    if !role.can_write() {
        return Err(ApiError::forbidden());
    }
    if !["personal", "team"].contains(&input.scope.as_str()) {
        return Err(ApiError::bad_request("invalid_image_scope"));
    }
    let service = image_provider(&state)?;
    let url = service_url(service, &format!("api/service/images/{image_id}/sharing"))?;
    proxy_json(
        provider_client()?
            .patch(url)
            .header("x-apex-service-key", service.key.as_str())
            .header("x-apex-user-id", user.id.to_string())
            .header("x-apex-team-id", team_id.to_string())
            .json(&json!({"scope": input.scope}))
            .send()
            .await,
    )
    .await
}

fn image_provider(state: &AppState) -> Result<&ServiceEndpoint, ApiError> {
    state.services.image_provider.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "image_hosting_not_configured",
        )
    })
}

fn invite_tracker(state: &AppState) -> Result<&ServiceEndpoint, ApiError> {
    state.services.invite_tracker.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "invite_tracking_not_configured",
        )
    })
}

fn service_url(service: &ServiceEndpoint, path: &str) -> Result<Url, ApiError> {
    service.origin.join(path).map_err(|_| ApiError::internal())
}

fn provider_client() -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "image_provider_unavailable"))
}

async fn proxy_json(
    response: Result<reqwest::Response, reqwest::Error>,
) -> Result<Response, ApiError> {
    let upstream = response
        .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "image_provider_unavailable"))?;
    let status = upstream.status();
    if status.is_server_error() || status == StatusCode::UNAUTHORIZED {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "image_provider_unavailable",
        ));
    }
    if upstream
        .content_length()
        .is_some_and(|length| length > MAX_PROVIDER_RESPONSE_BYTES as u64)
    {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "image_provider_response_too_large",
        ));
    }
    let mut stream = upstream.bytes_stream();
    let mut body = Vec::new();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "image_provider_unavailable"))?;
        if body.len().saturating_add(chunk.len()) > MAX_PROVIDER_RESPONSE_BYTES {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "image_provider_response_too_large",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{has_safe_service_transport, is_private_service_origin};
    use url::Url;

    fn private_http_origin(value: &str) -> bool {
        is_private_service_origin(&Url::parse(value).expect("valid test URL"))
    }

    #[test]
    fn allows_http_for_loopback_private_ip_and_local_service_names() {
        for origin in [
            "http://localhost:3000",
            "http://127.0.0.1:3000",
            "http://[::1]:3000",
            "http://[fe80::1]:3000",
            "http://10.0.0.12:3000",
            "http://172.20.0.4:3000",
            "http://192.168.1.12:3000",
            "http://image-provider:3000",
            "http://images.internal:3000",
            "http://tracker.local:3000",
        ] {
            assert!(
                private_http_origin(origin),
                "expected private origin: {origin}"
            );
        }
    }

    #[test]
    fn rejects_http_for_public_or_unspecified_hosts() {
        for origin in [
            "http://images.example.com",
            "http://203.0.113.10",
            "http://0.0.0.0:3000",
            "http://[::]:3000",
            "http://[2606:4700::1111]:3000",
        ] {
            let url = Url::parse(origin).expect("valid test URL");
            assert!(
                !has_safe_service_transport(&url),
                "expected public origin: {origin}"
            );
        }
    }

    #[test]
    fn allows_https_for_public_service_hosts() {
        let url = Url::parse("https://images.example.com").expect("valid test URL");

        assert!(has_safe_service_transport(&url));
    }
}
