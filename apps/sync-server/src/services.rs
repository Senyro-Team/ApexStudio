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
const SERVICE_PROBE_USER_ID: &str = "ffffffff-ffff-4fff-8fff-ffffffffffff";
const SERVICE_PROBE_TEAM_ID: &str = "ffffffff-ffff-4fff-8fff-fffffffffffe";

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
        endpoint_health(
            state.services.image_provider.as_ref(),
            "api/service/images?scope=available",
        ),
        endpoint_health(
            state.services.invite_tracker.as_ref(),
            "api/service/campaigns",
        ),
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
            .header("x-apex-service-key", service.key.as_str())
            .header("x-apex-user-id", SERVICE_PROBE_USER_ID)
            .header("x-apex-team-id", SERVICE_PROBE_TEAM_ID)
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
        provider_client("invite_tracking_unavailable")?
            .get(service_url(service, "api/service/campaigns")?)
            .header("x-apex-service-key", service.key.as_str())
            .header("x-apex-user-id", user.id.to_string())
            .header("x-apex-team-id", team_id.to_string())
            .send()
            .await,
        "invite_tracking_unavailable",
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
        provider_client("invite_tracking_unavailable")?
            .post(service_url(service, "api/service/campaigns")?)
            .header("content-type", content_type)
            .header("x-apex-service-key", service.key.as_str())
            .header("x-apex-user-id", user.id.to_string())
            .header("x-apex-team-id", team_id.to_string())
            .header("idempotency-key", request_id.to_string())
            .body(body)
            .send()
            .await,
        "invite_tracking_unavailable",
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
        provider_client("image_hosting_unavailable")?
            .get(url)
            .header("x-apex-service-key", service.key.as_str())
            .header("x-apex-user-id", user.id.to_string())
            .header("x-apex-team-id", team_id.to_string())
            .send()
            .await,
        "image_hosting_unavailable",
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
    let response = provider_client("image_hosting_unavailable")?
        .post(url)
        .header("content-type", content_type)
        .header("x-apex-service-key", service.key.as_str())
        .header("x-apex-user-id", user.id.to_string())
        .header("x-apex-team-id", team_id.to_string())
        .header("x-apex-scope", scope)
        .body(body)
        .send()
        .await;
    proxy_json(response, "image_hosting_unavailable").await
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
        provider_client("image_hosting_unavailable")?
            .patch(url)
            .header("x-apex-service-key", service.key.as_str())
            .header("x-apex-user-id", user.id.to_string())
            .header("x-apex-team-id", team_id.to_string())
            .json(&json!({"scope": input.scope}))
            .send()
            .await,
        "image_hosting_unavailable",
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

fn provider_client(unavailable_code: &'static str) -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, unavailable_code))
}

async fn proxy_json(
    response: Result<reqwest::Response, reqwest::Error>,
    unavailable_code: &'static str,
) -> Result<Response, ApiError> {
    let upstream =
        response.map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, unavailable_code))?;
    let status = upstream.status();
    if status.is_server_error() || status == StatusCode::UNAUTHORIZED {
        return Err(ApiError::new(StatusCode::BAD_GATEWAY, unavailable_code));
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
        let chunk = chunk.map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, unavailable_code))?;
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
    use super::{
        create_invite_campaign, has_safe_service_transport, is_private_service_origin, list_images,
        list_invite_campaigns, proxy_json, service_availability, set_image_scope, upload_images,
        ImageQuery, ServiceConfig, ShareRequest,
    };
    use crate::{
        auth::{ApiError, AuthUser},
        AppState,
    };
    use axum::{
        body::{to_bytes, Body},
        extract::{Extension, Path, Query, Request, State},
        http::StatusCode,
        response::{IntoResponse, Response},
        Json,
    };
    use sqlx::postgres::PgPoolOptions;
    use std::{
        env,
        io::{Read, Write},
        net::TcpListener,
        path::PathBuf,
        sync::Arc,
        thread,
        time::Duration,
    };
    use url::Url;
    use uuid::Uuid;

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

    #[tokio::test]
    async fn proxy_failures_report_the_service_that_was_unavailable() {
        for unavailable_code in ["image_hosting_unavailable", "invite_tracking_unavailable"] {
            let request_error = reqwest::Client::new()
                .get("not a valid URL")
                .send()
                .await
                .expect_err("invalid test URL must fail before connecting");
            let Err(error) = proxy_json(Err(request_error), unavailable_code).await else {
                panic!("failed upstream request must return an API error");
            };
            let response = error.into_response();

            assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
            let body = to_bytes(response.into_body(), 1024)
                .await
                .expect("small API error response");
            let body: serde_json::Value =
                serde_json::from_slice(&body).expect("valid JSON error response");
            assert_eq!(body["error"], unavailable_code);
        }
    }

    struct MembershipFixture {
        state: AppState,
        team_id: Uuid,
        other_team_id: Uuid,
        owner_id: Uuid,
        read_only_id: Uuid,
        designer_id: Uuid,
    }

    async fn membership_fixture() -> Option<MembershipFixture> {
        let database_url = match env::var("APEX_SYNC_TEST_DATABASE_URL") {
            Ok(value) => value,
            Err(_) => {
                eprintln!("skipping database-backed service authorization test; APEX_SYNC_TEST_DATABASE_URL is not set");
                return None;
            }
        };
        let database = PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
            .expect("test PostgreSQL service should be reachable");
        sqlx::query(
            "CREATE TEMPORARY TABLE team_memberships (team_id UUID NOT NULL, user_id UUID NOT NULL, role TEXT NOT NULL, PRIMARY KEY(team_id, user_id))",
        )
        .execute(&database)
        .await
        .expect("temporary membership table should be created");

        let team_id = Uuid::now_v7();
        let other_team_id = Uuid::now_v7();
        let owner_id = Uuid::now_v7();
        let read_only_id = Uuid::now_v7();
        let designer_id = Uuid::now_v7();
        for (user_id, role) in [
            (owner_id, "owner"),
            (read_only_id, "read_only"),
            (designer_id, "designer"),
        ] {
            sqlx::query("INSERT INTO team_memberships(team_id,user_id,role) VALUES ($1,$2,$3)")
                .bind(team_id)
                .bind(user_id)
                .bind(role)
                .execute(&database)
                .await
                .expect("team membership should be inserted");
        }

        Some(MembershipFixture {
            state: AppState {
                database,
                bootstrap_token_hash: None,
                password_hash_slots: Arc::new(tokio::sync::Semaphore::new(1)),
                storage_dir: PathBuf::from("test-storage"),
                max_blob_storage_bytes: 1024,
                services: ServiceConfig::default(),
            },
            team_id,
            other_team_id,
            owner_id,
            read_only_id,
            designer_id,
        })
    }

    fn assert_error_status<T>(result: Result<T, ApiError>, expected: StatusCode) {
        let Err(error) = result else {
            panic!("expected API error with status {expected}");
        };
        assert_eq!(error.into_response().status(), expected);
    }

    fn campaign_request() -> Request {
        Request::builder()
            .method("POST")
            .header("content-type", "application/json")
            .header("idempotency-key", Uuid::now_v7().to_string())
            .body(Body::from("{}"))
            .expect("valid test request")
    }

    fn mock_service(body: &'static str) -> (super::ServiceEndpoint, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock service should bind");
        let address = listener
            .local_addr()
            .expect("mock service should have an address");
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("gateway should connect");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("mock socket timeout should be set");
            let mut buffer = [0; 8192];
            let length = stream
                .read(&mut buffer)
                .expect("request should be readable");
            let request = String::from_utf8_lossy(&buffer[..length]).into_owned();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nx-provider-private: do-not-forward\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("mock response should be writable");
            request
        });
        (
            super::ServiceEndpoint {
                origin: Url::parse(&format!("http://{address}"))
                    .expect("mock origin should be valid"),
                key: zeroize::Zeroizing::new("service-key-never-exposed-1234567890".to_owned()),
            },
            worker,
        )
    }

    fn assert_scoped_gateway_request(
        request: &str,
        method: &str,
        path: &str,
        user_id: Uuid,
        team_id: Uuid,
    ) {
        let request = request.to_ascii_lowercase();
        let request_line = request.lines().next().unwrap_or_default();
        assert!(
            request_line.starts_with(&format!("{} ", method.to_ascii_lowercase()))
                && request_line.contains(path)
                && request_line.ends_with("http/1.1"),
            "unexpected upstream request line: {request_line}"
        );
        assert!(request.contains("x-apex-service-key: service-key-never-exposed-1234567890"));
        assert!(request.contains(&format!("x-apex-user-id: {user_id}")));
        assert!(request.contains(&format!("x-apex-team-id: {team_id}")));
    }

    async fn assert_provider_response_is_sanitized(response: Response, body: &str) {
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get("x-apex-service-key").is_none());
        assert!(response.headers().get("x-provider-private").is_none());
        let bytes = to_bytes(response.into_body(), 1024)
            .await
            .expect("small provider response");
        assert_eq!(bytes, body.as_bytes());
        assert!(!String::from_utf8_lossy(&bytes).contains("service-key-never-exposed"));
    }

    #[tokio::test]
    async fn read_gateways_forward_scoped_identity_and_keep_service_secrets_private() {
        let Some(mut fixture) = membership_fixture().await else {
            return;
        };
        let user_id = fixture.owner_id;
        let team_id = fixture.team_id;
        let (image_provider, image_worker) = mock_service(r#"{"items":[]}"#);
        let (invite_tracker, invite_worker) = mock_service(r#"{"campaigns":[]}"#);
        fixture.state.services = ServiceConfig {
            image_provider: Some(image_provider),
            invite_tracker: Some(invite_tracker),
        };

        let image_response = match list_images(
            State(fixture.state.clone()),
            Extension(AuthUser { id: user_id }),
            Path(team_id),
            Query(ImageQuery {
                scope: "team".to_owned(),
            }),
        )
        .await
        {
            Ok(response) => response,
            Err(_) => panic!("authorized image listing should be proxied"),
        };
        assert_provider_response_is_sanitized(image_response, r#"{"items":[]}"#).await;
        let image_request = image_worker
            .join()
            .expect("mock image provider thread should finish");
        assert_scoped_gateway_request(
            &image_request,
            "GET",
            "/api/service/images?scope=team",
            user_id,
            team_id,
        );

        let invite_response = match list_invite_campaigns(
            State(fixture.state),
            Extension(AuthUser { id: user_id }),
            Path(team_id),
        )
        .await
        {
            Ok(response) => response,
            Err(_) => panic!("authorized invite listing should be proxied"),
        };
        assert_provider_response_is_sanitized(invite_response, r#"{"campaigns":[]}"#).await;
        let invite_request = invite_worker
            .join()
            .expect("mock invite tracker thread should finish");
        assert_scoped_gateway_request(
            &invite_request,
            "GET",
            "/api/service/campaigns",
            user_id,
            team_id,
        );
    }

    #[tokio::test]
    async fn write_gateways_forward_scope_and_idempotency_to_the_correct_service() {
        let Some(mut fixture) = membership_fixture().await else {
            return;
        };
        let user_id = fixture.designer_id;
        let team_id = fixture.team_id;
        let image_id = Uuid::now_v7();
        let (upload_provider, upload_worker) = mock_service(r#"{"ok":true}"#);
        let (share_provider, share_worker) = mock_service(r#"{"ok":true}"#);
        let (invite_tracker, invite_worker) = mock_service(r#"{"ok":true}"#);
        fixture.state.services = ServiceConfig {
            image_provider: Some(upload_provider),
            invite_tracker: Some(invite_tracker),
        };

        let upload_request = Request::builder()
            .method("POST")
            .uri("/?scope=team")
            .header(
                "content-type",
                "multipart/form-data; boundary=test-boundary",
            )
            .body(Body::from("--test-boundary--\r\n"))
            .expect("valid image upload request");
        let upload_response = match upload_images(
            State(fixture.state.clone()),
            Extension(AuthUser { id: user_id }),
            Path(team_id),
            upload_request,
        )
        .await
        {
            Ok(response) => response,
            Err(_) => panic!("authorized image upload should be proxied"),
        };
        assert_provider_response_is_sanitized(upload_response, r#"{"ok":true}"#).await;
        let upload_request = upload_worker
            .join()
            .expect("mock image provider thread should finish")
            .to_ascii_lowercase();
        assert_scoped_gateway_request(
            &upload_request,
            "POST",
            "/api/service/images",
            user_id,
            team_id,
        );
        assert!(upload_request.contains("x-apex-scope: team"));
        assert!(
            upload_request.contains("content-type: multipart/form-data; boundary=test-boundary")
        );

        fixture.state.services.image_provider = Some(share_provider);
        let share_response = match set_image_scope(
            State(fixture.state.clone()),
            Extension(AuthUser { id: user_id }),
            Path((team_id, image_id)),
            Json(ShareRequest {
                scope: "team".to_owned(),
            }),
        )
        .await
        {
            Ok(response) => response,
            Err(_) => panic!("authorized image sharing update should be proxied"),
        };
        assert_provider_response_is_sanitized(share_response, r#"{"ok":true}"#).await;
        let share_request = share_worker
            .join()
            .expect("mock image provider thread should finish")
            .to_ascii_lowercase();
        assert_scoped_gateway_request(
            &share_request,
            "PATCH",
            &format!("/api/service/images/{image_id}/sharing"),
            user_id,
            team_id,
        );
        assert!(share_request.ends_with(r#"{"scope":"team"}"#));

        let request_id = Uuid::now_v7();
        let invite_request = Request::builder()
            .method("POST")
            .header("content-type", "application/json")
            .header("idempotency-key", request_id.to_string())
            .body(Body::from(r#"{"title":"Test campaign"}"#))
            .expect("valid campaign request");
        let invite_response = match create_invite_campaign(
            State(fixture.state),
            Extension(AuthUser { id: user_id }),
            Path(team_id),
            invite_request,
        )
        .await
        {
            Ok(response) => response,
            Err(_) => panic!("authorized campaign creation should be proxied"),
        };
        assert_provider_response_is_sanitized(invite_response, r#"{"ok":true}"#).await;
        let invite_request = invite_worker
            .join()
            .expect("mock invite tracker thread should finish")
            .to_ascii_lowercase();
        assert_scoped_gateway_request(
            &invite_request,
            "POST",
            "/api/service/campaigns",
            user_id,
            team_id,
        );
        assert!(invite_request.contains(&format!("idempotency-key: {request_id}")));
        assert!(invite_request.contains(r#"{"title":"test campaign"}"#));
    }

    #[tokio::test]
    async fn service_gateway_enforces_team_membership_and_write_roles() {
        let Some(fixture) = membership_fixture().await else {
            return;
        };
        let MembershipFixture {
            state,
            team_id,
            other_team_id,
            owner_id,
            read_only_id,
            designer_id,
        } = fixture;

        let nonmember = AuthUser { id: owner_id };
        assert_error_status(
            service_availability(
                State(state.clone()),
                Extension(nonmember.clone()),
                Path(other_team_id),
            )
            .await
            .map(|_| ()),
            StatusCode::FORBIDDEN,
        );
        assert_error_status(
            list_invite_campaigns(
                State(state.clone()),
                Extension(nonmember.clone()),
                Path(other_team_id),
            )
            .await,
            StatusCode::FORBIDDEN,
        );
        assert_error_status(
            list_images(
                State(state.clone()),
                Extension(nonmember),
                Path(other_team_id),
                Query(ImageQuery {
                    scope: "available".to_owned(),
                }),
            )
            .await,
            StatusCode::FORBIDDEN,
        );

        let read_only = Extension(AuthUser { id: read_only_id });
        assert_error_status(
            list_invite_campaigns(State(state.clone()), read_only.clone(), Path(team_id)).await,
            StatusCode::SERVICE_UNAVAILABLE,
        );
        assert_error_status(
            list_images(
                State(state.clone()),
                read_only.clone(),
                Path(team_id),
                Query(ImageQuery {
                    scope: "available".to_owned(),
                }),
            )
            .await,
            StatusCode::SERVICE_UNAVAILABLE,
        );
        assert_error_status(
            create_invite_campaign(
                State(state.clone()),
                read_only.clone(),
                Path(team_id),
                campaign_request(),
            )
            .await,
            StatusCode::FORBIDDEN,
        );
        assert_error_status(
            upload_images(
                State(state.clone()),
                read_only.clone(),
                Path(team_id),
                Request::new(Body::empty()),
            )
            .await,
            StatusCode::FORBIDDEN,
        );
        assert_error_status(
            set_image_scope(
                State(state.clone()),
                read_only,
                Path((team_id, Uuid::now_v7())),
                Json(ShareRequest {
                    scope: "team".to_owned(),
                }),
            )
            .await,
            StatusCode::FORBIDDEN,
        );

        let designer = Extension(AuthUser { id: designer_id });
        assert_error_status(
            create_invite_campaign(
                State(state.clone()),
                designer.clone(),
                Path(team_id),
                campaign_request(),
            )
            .await,
            StatusCode::SERVICE_UNAVAILABLE,
        );
        assert_error_status(
            upload_images(
                State(state.clone()),
                designer.clone(),
                Path(team_id),
                Request::new(Body::empty()),
            )
            .await,
            StatusCode::SERVICE_UNAVAILABLE,
        );
        assert_error_status(
            set_image_scope(
                State(state),
                designer,
                Path((team_id, Uuid::now_v7())),
                Json(ShareRequest {
                    scope: "team".to_owned(),
                }),
            )
            .await,
            StatusCode::SERVICE_UNAVAILABLE,
        );
    }
}
