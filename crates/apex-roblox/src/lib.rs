//! Roblox Open Cloud Assets adapter. Live requests are opt-in; tests use local HTTP fixtures.

use std::{fmt, io::Read, time::Duration};

use apex_domain::{
    AppError, ExternalRobloxId, IdentifierResolutionStatus, IdentifierSource,
    RobloxAssetIdentifier, RobloxIdentifierKind,
};
use chrono::Utc;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const OFFICIAL_ORIGIN: &str = "https://apis.roblox.com";
pub const MAX_IMAGE_BYTES: usize = 20_000_000;
const MAX_RESPONSE_BYTES: u64 = 1_048_576;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreatorKind {
    User,
    Group,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CreatorProfile {
    pub id: String,
    pub display_name: String,
    pub kind: CreatorKind,
    pub creator_id: ExternalRobloxId,
    pub secret_reference: String,
}

impl CreatorProfile {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.display_name.trim().is_empty() || self.display_name.len() > 120 {
            return Err(AppError::validation(
                "roblox.profile.name_invalid",
                "Das Profil benötigt einen Anzeigenamen mit höchstens 120 Zeichen.",
            ));
        }
        if self.id.is_empty()
            || self.id.chars().any(char::is_control)
            || self.secret_reference.is_empty()
        {
            return Err(AppError::validation(
                "roblox.profile.invalid",
                "Das Roblox-Profil ist unvollständig.",
            ));
        }
        Ok(())
    }
}

pub trait SecretStore: Send + Sync {
    fn put(&self, reference: &str, secret: &str) -> Result<(), AppError>;
    fn get(&self, reference: &str) -> Result<String, AppError>;
    fn delete(&self, reference: &str) -> Result<(), AppError>;
}

/// Uses the platform credential store exposed by keyring. No plaintext fallback is provided.
pub struct OsSecretStore;

impl OsSecretStore {
    fn entry(reference: &str) -> Result<keyring::Entry, AppError> {
        if reference.is_empty() || reference.len() > 160 || reference.chars().any(char::is_control)
        {
            return Err(AppError::validation(
                "roblox.secret_reference.invalid",
                "Die Schlüsselreferenz ist ungültig.",
            ));
        }
        keyring::Entry::new("Apex Studio / Roblox Open Cloud", reference).map_err(|_| {
            AppError::configuration(
                "roblox.secret_store_unavailable",
                "Der Betriebssystem-Schlüsselspeicher ist nicht verfügbar.",
            )
        })
    }
}

impl SecretStore for OsSecretStore {
    fn put(&self, reference: &str, secret: &str) -> Result<(), AppError> {
        if secret.trim().is_empty() {
            return Err(missing_credentials());
        }
        Self::entry(reference)?.set_password(secret).map_err(|_| AppError::configuration("roblox.secret_store_unavailable", "Der Schlüssel konnte nicht im Betriebssystem-Schlüsselspeicher gespeichert werden."))
    }
    fn get(&self, reference: &str) -> Result<String, AppError> {
        Self::entry(reference)?
            .get_password()
            .map_err(|_| missing_credentials())
    }
    fn delete(&self, reference: &str) -> Result<(), AppError> {
        let entry = Self::entry(reference)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::configuration(
                "roblox.secret_store_unavailable",
                "Der Schlüssel konnte nicht aus dem Betriebssystem-Schlüsselspeicher entfernt werden.",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum UploadAssetType {
    Image,
    Decal,
}

impl UploadAssetType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Image => "Image",
            Self::Decal => "Decal",
        }
    }
}

#[derive(Clone, Debug)]
pub struct UploadIntent {
    pub creator: CreatorProfile,
    pub asset_type: UploadAssetType,
    pub sha256: String,
    pub png_bytes: Vec<u8>,
}

impl UploadIntent {
    pub fn validate(&self) -> Result<(), AppError> {
        self.creator.validate()?;
        if self.png_bytes.is_empty() || self.png_bytes.len() > MAX_IMAGE_BYTES {
            return Err(AppError::validation(
                "image.size.invalid",
                "PNG muss zwischen 1 und 20.000.000 Bytes groß sein.",
            ));
        }
        let actual = hex(&Sha256::digest(&self.png_bytes));
        if self.sha256 != actual {
            return Err(AppError::validation(
                "image.hash_mismatch",
                "Der Uploadblob stimmt nicht mit dem geprüften SHA-256 überein.",
            ));
        }
        apex_images::inspect_png(&self.png_bytes).map_err(|_| AppError::validation("provider_validation", "Der Roblox-Adapter akzeptiert nur vollständig dekodierbare PNG-Dateien aus der lokalen Bildprüfung."))?;
        Ok(())
    }
    pub fn display_name(&self) -> String {
        format!("APXSTD-{}", self.sha256.to_ascii_uppercase())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RemoteOperation {
    pub operation_id: String,
    pub done: bool,
    pub asset_id: Option<ExternalRobloxId>,
    pub asset_type: Option<String>,
    pub moderation_state: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AssetMetadata {
    pub asset_id: ExternalRobloxId,
    pub asset_type: String,
    pub creator_id: Option<String>,
    pub moderation_state: Option<String>,
}

/// Classifies only the returned Asset type. The other ID remains unavailable unless Roblox documents a relation.
pub fn identifiers_from_operation(
    operation: &RemoteOperation,
    creator_id: &str,
) -> [RobloxAssetIdentifier; 2] {
    let returned = match operation.asset_type.as_deref() {
        Some("Image" | "ASSET_TYPE_IMAGE") => Some(RobloxIdentifierKind::Image),
        Some("Decal" | "ASSET_TYPE_DECAL") => Some(RobloxIdentifierKind::Decal),
        _ => None,
    };
    let make = |kind| {
        let value = if returned == Some(kind) {
            operation.asset_id.clone()
        } else {
            None
        };
        let resolved = operation.done && value.is_some();
        RobloxAssetIdentifier {
            kind,
            value,
            resolution_status: if resolved { IdentifierResolutionStatus::Resolved } else if !operation.done { IdentifierResolutionStatus::Pending } else if returned.is_none() { IdentifierResolutionStatus::Unknown } else { IdentifierResolutionStatus::Unavailable },
            source: resolved.then_some(IdentifierSource::UploadResponse),
            verified_at: resolved.then(Utc::now),
            creator_id: Some(creator_id.to_owned()),
            note: (returned != Some(kind)).then(|| "Roblox hat für dieses Asset keine entsprechend typisierte ID zurückgegeben; eine Beziehung ist nicht dokumentiert.".to_owned()),
        }
    };
    [
        make(RobloxIdentifierKind::Image),
        make(RobloxIdentifierKind::Decal),
    ]
}

pub trait RobloxProvider: Send + Sync {
    fn start_upload(&self, intent: &UploadIntent) -> Result<RemoteOperation, AppError>;
    fn get_operation(
        &self,
        creator: &CreatorProfile,
        operation_id: &str,
    ) -> Result<RemoteOperation, AppError>;
    fn get_asset_metadata(
        &self,
        creator: &CreatorProfile,
        asset_id: &ExternalRobloxId,
    ) -> Result<AssetMetadata, AppError>;
}

pub struct RobloxOpenCloud<S: SecretStore> {
    client: Client,
    secrets: S,
    origin: String,
}

impl<S: SecretStore> RobloxOpenCloud<S> {
    pub fn live(secrets: S) -> Result<Self, AppError> {
        Self::at_origin(secrets, OFFICIAL_ORIGIN)
    }

    fn at_origin(secrets: S, origin: &str) -> Result<Self, AppError> {
        let parsed = reqwest::Url::parse(origin).map_err(|_| unavailable())?;
        let is_official =
            parsed.scheme() == "https" && parsed.host_str() == Some("apis.roblox.com");
        let is_local_sim = cfg!(test)
            && parsed.scheme() == "http"
            && matches!(parsed.host_str(), Some("127.0.0.1") | Some("localhost"));
        if !is_official && !is_local_sim {
            return Err(AppError::configuration(
                "roblox.origin_invalid",
                "Der Roblox-Origin ist nicht zugelassen.",
            ));
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .build()
            .map_err(|_| unavailable())?;
        Ok(Self {
            client,
            secrets,
            origin: parsed.as_str().trim_end_matches('/').to_owned(),
        })
    }

    fn api_key(&self, profile: &CreatorProfile) -> Result<String, AppError> {
        profile.validate()?;
        let secret = self.secrets.get(&profile.secret_reference)?;
        if secret.trim().is_empty() {
            Err(missing_credentials())
        } else {
            Ok(secret)
        }
    }

    fn json_response(&self, response: Response) -> Result<serde_json::Value, AppError> {
        if response.status().is_redirection() {
            return Err(provider_error(
                "provider_rejected",
                "Roblox hat eine Weiterleitung gesendet.",
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err(provider_error(
                "provider_validation",
                "Die Roblox-Antwort überschreitet das Größenlimit.",
            ));
        }
        let status = response.status();
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| unavailable())?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(provider_error(
                "provider_validation",
                "Die Roblox-Antwort überschreitet das Größenlimit.",
            ));
        }
        if !status.is_success() {
            let (code, message) = match status.as_u16() {
                401 => (
                    "auth_expired",
                    "Der Roblox-Schlüssel wurde abgelehnt oder ist abgelaufen.",
                ),
                403 => (
                    "forbidden_creator",
                    "Der Schlüssel darf für diesen Creator nicht auf die Assets zugreifen.",
                ),
                429 => (
                    "rate_limited",
                    "Roblox begrenzt die Anfragerate; später erneut prüfen.",
                ),
                402 => (
                    "quota_exceeded",
                    "Roblox meldet ein Quota- oder Kontingentproblem.",
                ),
                400 | 413 | 415 | 422 => (
                    "provider_validation",
                    "Roblox hat Assettyp, Metadaten oder Inhalt abgelehnt.",
                ),
                500..=599 => ("unavailable", "Roblox ist momentan nicht verfügbar."),
                _ => ("provider_rejected", "Roblox hat die Anfrage abgelehnt."),
            };
            let mut error = provider_error(code, message);
            error.details = request_id.map(|id| format!("request_id={}", sanitize(&id)));
            return Err(error);
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            provider_error(
                "provider_validation",
                "Roblox hat eine ungültige JSON-Antwort gesendet.",
            )
        })
    }

    fn operation(
        &self,
        operation_id: &str,
        profile: &CreatorProfile,
    ) -> Result<RemoteOperation, AppError> {
        let operation_id = normalize_operation_id(operation_id)?;
        let key = self.api_key(profile)?;
        let response = self
            .client
            .get(format!(
                "{}/assets/v1/operations/{}",
                self.origin, operation_id
            ))
            .header("x-api-key", key)
            .send()
            .map_err(|_| unknown_outcome())?;
        let body = self.json_response(response)?;
        parse_operation(body, &operation_id)
    }
}

impl<S: SecretStore> RobloxProvider for RobloxOpenCloud<S> {
    fn start_upload(&self, intent: &UploadIntent) -> Result<RemoteOperation, AppError> {
        intent.validate()?;
        let image = apex_images::inspect_png(&intent.png_bytes).map_err(|_| {
            AppError::validation(
                "provider_validation",
                "Die PNG-Datei ist nicht lokal freigegeben.",
            )
        })?;
        if provider_image_warning(image.width, image.height).is_some() {
            return Err(provider_error(
                "provider_validation",
                "Roblox verlangt für Decal/Image weniger als 8000 Pixel je Seite. Apex hat die Datei nicht verändert; bitte eine kleinere Fassung vorbereiten.",
            ));
        }
        let key = self.api_key(&intent.creator)?;
        let creator = match intent.creator.kind {
            CreatorKind::User => serde_json::json!({"userId": intent.creator.creator_id.as_str()}),
            CreatorKind::Group => {
                serde_json::json!({"groupId": intent.creator.creator_id.as_str()})
            }
        };
        let request = serde_json::json!({
            "assetType": intent.asset_type.as_str(),
            "displayName": intent.display_name(),
            "description": "Uploaded with Apex Studio",
            "creationContext": {"creator": creator}
        });
        let metadata = reqwest::blocking::multipart::Part::text(request.to_string())
            .mime_str("application/json")
            .map_err(|_| unavailable())?;
        let file = reqwest::blocking::multipart::Part::bytes(intent.png_bytes.clone())
            .file_name("asset.png")
            .mime_str("image/png")
            .map_err(|_| unavailable())?;
        let form = reqwest::blocking::multipart::Form::new()
            .part("request", metadata)
            .part("fileContent", file);
        let response = self
            .client
            .post(format!("{}/assets/v1/assets", self.origin))
            .header("x-api-key", key)
            .multipart(form)
            .send()
            .map_err(|_| unknown_outcome())?;
        let mut body = self.json_response(response)?;
        let path = body
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                provider_error(
                    "provider_validation",
                    "Roblox hat keinen Operationpfad zurückgegeben.",
                )
            })?;
        let operation_id = normalize_operation_path(path)?;
        // Create Asset may acknowledge with only its operation path; polling supplies `done`.
        if body.get("done").is_none() {
            body.as_object_mut()
                .ok_or_else(|| {
                    provider_error(
                        "provider_validation",
                        "Roblox hat eine ungültige Uploadantwort gesendet.",
                    )
                })?
                .insert("done".into(), serde_json::Value::Bool(false));
        }
        parse_operation(body, &operation_id)
    }

    fn get_operation(
        &self,
        creator: &CreatorProfile,
        operation_id: &str,
    ) -> Result<RemoteOperation, AppError> {
        self.operation(operation_id, creator)
    }

    fn get_asset_metadata(
        &self,
        profile: &CreatorProfile,
        asset_id: &ExternalRobloxId,
    ) -> Result<AssetMetadata, AppError> {
        let key = self.api_key(profile)?;
        let response = self
            .client
            .get(format!(
                "{}/assets/v1/assets/{}",
                self.origin,
                asset_id.as_str()
            ))
            .header("x-api-key", key)
            .send()
            .map_err(|_| unavailable())?;
        let body = self.json_response(response)?;
        parse_asset_metadata(body, asset_id.clone())
    }
}

fn parse_operation(body: serde_json::Value, id: &str) -> Result<RemoteOperation, AppError> {
    let done = body
        .get("done")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            provider_error(
                "provider_validation",
                "Roblox-Operation enthält keinen gültigen Status.",
            )
        })?;
    let result = body.get("response").unwrap_or(&serde_json::Value::Null);
    let asset_id = result
        .get("assetId")
        .and_then(serde_json::Value::as_str)
        .map(ExternalRobloxId::parse)
        .transpose()?;
    Ok(RemoteOperation {
        operation_id: id.to_owned(),
        done,
        asset_id,
        asset_type: result
            .get("assetType")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        moderation_state: result
            .get("moderationResult")
            .and_then(|v| v.get("moderationState"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    })
}

fn parse_asset_metadata(
    body: serde_json::Value,
    id: ExternalRobloxId,
) -> Result<AssetMetadata, AppError> {
    let asset_type = body
        .get("assetType")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            provider_error(
                "provider_validation",
                "Roblox-Assetmetadaten enthalten keinen Assettyp.",
            )
        })?
        .to_owned();
    let creator_id = body
        .pointer("/creationContext/creator/userId")
        .or_else(|| body.pointer("/creationContext/creator/groupId"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let moderation_state = body
        .get("moderationResult")
        .and_then(|v| v.get("moderationState"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok(AssetMetadata {
        asset_id: id,
        asset_type,
        creator_id,
        moderation_state,
    })
}

pub fn normalize_operation_path(path: &str) -> Result<String, AppError> {
    let Some(id) = path.strip_prefix("operations/") else {
        return Err(provider_error(
            "provider_validation",
            "Roblox hat einen ungültigen Operationpfad gesendet.",
        ));
    };
    normalize_operation_id(id)
}

fn normalize_operation_id(id: &str) -> Result<String, AppError> {
    if id.is_empty()
        || id.len() > 160
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(provider_error(
            "provider_validation",
            "Roblox hat eine ungültige Operation-ID gesendet.",
        ));
    }
    Ok(id.to_owned())
}

pub fn provider_image_warning(width: u32, height: u32) -> Option<&'static str> {
    (width >= 8000 || height >= 8000).then_some("Roblox verlangt für Decal/Image eine Bildgröße kleiner als 8000×8000; die lokale Apex-Grenze erlaubt einschließlich 8000.")
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(80)
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn missing_credentials() -> AppError {
    AppError::configuration(
        "credentials_missing",
        "Für dieses Roblox-Profil fehlt ein Open-Cloud-API-Key im Betriebssystem-Schlüsselspeicher.",
    )
}
fn unavailable() -> AppError {
    AppError {
        code: "unavailable".into(),
        message: "Roblox Open Cloud ist momentan nicht verfügbar.".into(),
        retryable: true,
        details: None,
    }
}
fn unknown_outcome() -> AppError {
    AppError { code: "transport_unknown_outcome".into(), message: "Der Verbindungsabbruch lässt den Roblox-Ausgang offen; nicht automatisch erneut hochladen.".into(), retryable: false, details: None }
}
fn provider_error(code: &str, message: &str) -> AppError {
    AppError {
        code: code.into(),
        message: message.into(),
        retryable: matches!(code, "rate_limited" | "unavailable"),
        details: None,
    }
}

impl<S: SecretStore> fmt::Debug for RobloxOpenCloud<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RobloxOpenCloud")
            .field("origin", &self.origin)
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };

    #[derive(Clone, Default)]
    struct MemorySecrets(Arc<Mutex<HashMap<String, String>>>);
    impl SecretStore for MemorySecrets {
        fn put(&self, r: &str, s: &str) -> Result<(), AppError> {
            self.0.lock().unwrap().insert(r.into(), s.into());
            Ok(())
        }
        fn get(&self, r: &str) -> Result<String, AppError> {
            self.0
                .lock()
                .unwrap()
                .get(r)
                .cloned()
                .ok_or_else(missing_credentials)
        }
        fn delete(&self, r: &str) -> Result<(), AppError> {
            self.0.lock().unwrap().remove(r);
            Ok(())
        }
    }
    fn profile() -> CreatorProfile {
        CreatorProfile {
            id: "test-profile".into(),
            display_name: "Studio".into(),
            kind: CreatorKind::Group,
            creator_id: ExternalRobloxId::parse("9007199254740993").unwrap(),
            secret_reference: "test-key".into(),
        }
    }
    fn intent() -> UploadIntent {
        let bytes = include_bytes!("../../../apps/desktop/src-tauri/icons/StoreLogo.png").to_vec();
        UploadIntent {
            creator: profile(),
            asset_type: UploadAssetType::Decal,
            sha256: hex(&Sha256::digest(&bytes)),
            png_bytes: bytes,
        }
    }

    #[test]
    fn operation_paths_are_strictly_relative_and_ids_are_strings() {
        assert_eq!(
            normalize_operation_path("operations/op_123").unwrap(),
            "op_123"
        );
        for bad in [
            "https://evil.test/operations/x",
            "operations/../x",
            "/operations/x",
            "operations/x?a=1",
        ] {
            assert_eq!(
                normalize_operation_path(bad).unwrap_err().code,
                "provider_validation"
            );
        }
        assert_eq!(profile().creator_id.as_str(), "9007199254740993");
    }

    #[test]
    fn upload_contract_uses_hash_creator_and_neutral_name() {
        let job = intent();
        job.validate().unwrap();
        assert!(job.display_name().starts_with("APXSTD-"));
        assert_eq!(provider_image_warning(7999, 7999), None);
        assert!(provider_image_warning(8000, 1).is_some());
        let mut changed = job.clone();
        changed.sha256 = "0".repeat(64);
        assert_eq!(changed.validate().unwrap_err().code, "image.hash_mismatch");
    }

    #[test]
    fn simulator_fixture_uses_same_operation_adapter_without_redirects() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0u8; 8192];
            let n = socket.read(&mut request).unwrap();
            let text = String::from_utf8_lossy(&request[..n]);
            assert!(text.contains("GET /assets/v1/operations/op_fixture"));
            assert!(
                text.to_ascii_lowercase()
                    .contains("x-api-key: simulator-secret")
            );
            let body = r#"{"done":true,"response":{"assetId":"9007199254740993","assetType":"Decal","moderationResult":{"moderationState":"Approved"}}}"#;
            use std::io::Write;
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        let secrets = MemorySecrets::default();
        secrets.put("test-key", "simulator-secret").unwrap();
        let adapter = RobloxOpenCloud::at_origin(secrets, &format!("http://{address}")).unwrap();
        let result = adapter.operation("op_fixture", &profile()).unwrap();
        server.join().unwrap();
        assert!(result.done);
        assert_eq!(result.asset_id.unwrap().as_str(), "9007199254740993");
        assert_eq!(result.moderation_state.as_deref(), Some("Approved"));
    }

    #[test]
    fn simulator_upload_fixture_checks_multipart_and_never_sends_source_name() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut one = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut one).unwrap();
                request.push(one[0]);
            }
            let headers = String::from_utf8_lossy(&request).to_ascii_lowercase();
            assert!(headers.contains("post /assets/v1/assets"));
            assert!(headers.contains("x-api-key: simulator-secret"));
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length: ")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                })
                .unwrap();
            let mut body = vec![0u8; content_length];
            socket.read_exact(&mut body).unwrap();
            let request = String::from_utf8_lossy(&body);
            assert!(request.contains("\"assetType\":\"Decal\""));
            assert!(request.contains("\"groupId\":\"9007199254740993\""));
            assert!(request.contains("APXSTD-"));
            assert!(request.contains("filename=\"asset.png\""));
            assert!(!request.contains("private-customer-filename.png"));
            let response = r#"{"path":"operations/op_create"}"#;
            use std::io::Write;
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
        });
        let secrets = MemorySecrets::default();
        secrets.put("test-key", "simulator-secret").unwrap();
        let adapter = RobloxOpenCloud::at_origin(secrets, &format!("http://{address}")).unwrap();
        let job = intent();
        let result = adapter.start_upload(&job).unwrap();
        server.join().unwrap();
        assert_eq!(result.operation_id, "op_create");
        assert!(!result.done);
    }

    #[test]
    fn secret_never_appears_in_adapter_debug_or_error() {
        let secrets = MemorySecrets::default();
        secrets
            .put("test-key", "test-secret-should-not-leak")
            .unwrap();
        let adapter = RobloxOpenCloud::at_origin(secrets, OFFICIAL_ORIGIN).unwrap();
        assert!(!format!("{adapter:?}").contains("test-secret"));
        assert!(!missing_credentials().message.contains("test-secret"));
    }

    #[test]
    fn operation_classifies_only_the_returned_asset_type() {
        let operation = RemoteOperation {
            operation_id: "op".into(),
            done: true,
            asset_id: Some(ExternalRobloxId::parse("9007199254740993").unwrap()),
            asset_type: Some("ASSET_TYPE_DECAL".into()),
            moderation_state: Some("Approved".into()),
        };
        let ids = identifiers_from_operation(&operation, "42");
        assert_eq!(
            ids[0].resolution_status,
            IdentifierResolutionStatus::Unavailable
        );
        assert!(ids[0].value.is_none());
        assert_eq!(
            ids[1].resolution_status,
            IdentifierResolutionStatus::Resolved
        );
        assert_eq!(ids[1].value.as_ref().unwrap().as_str(), "9007199254740993");
        assert_eq!(ids[1].source, Some(IdentifierSource::UploadResponse));
    }
}
