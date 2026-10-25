use super::{projects::work, CommandError};
use apex_core::Error;
use apex_storage::{DecalUpload, LiveryUploads, Workspace};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Response,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use zeroize::{Zeroize, Zeroizing};

#[derive(Default)]
pub struct RobloxState(pub tokio::sync::Mutex<()>);
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    account_id: String,
    api_key: String,
}
impl Drop for Credentials {
    fn drop(&mut self) {
        self.account_id.zeroize();
        self.api_key.zeroize();
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RobloxSettings {
    configured: bool,
    account_id: Option<String>,
}
fn bad(message: &str) -> Error {
    Error::InvalidContent(message.into())
}
fn vault() -> Result<keyring::Entry, Error> {
    if !cfg!(target_os = "windows") {
        return Err(bad(
            "Secure Roblox settings are currently available on Windows.",
        ));
    }
    keyring::Entry::new("studio.apexdesigns.apexstudio", "roblox-personal")
        .map_err(|_| bad("Windows Credential Manager is unavailable."))
}
fn load() -> Result<Option<Credentials>, Error> {
    match vault()?.get_secret() {
        Ok(bytes) => {
            let bytes = Zeroizing::new(bytes);
            serde_json::from_slice(&bytes).map(Some).map_err(|_| {
                bad("Saved Roblox credentials cannot be read. Replace them in Settings.")
            })
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(bad("Windows could not unlock Roblox credentials.")),
    }
}
fn required() -> Result<Credentials, Error> {
    load()?.ok_or_else(|| bad("Configure Roblox in Settings before uploading."))
}
fn numeric(s: &str) -> bool {
    !s.is_empty() && s.len() <= 20 && !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit())
}
fn client() -> Result<Client, Error> {
    Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|_| bad("Could not start the Roblox connection."))
}
fn headers(c: &Credentials) -> Result<HeaderMap, Error> {
    let mut value = HeaderValue::from_str(&c.api_key)
        .map_err(|_| bad("The API key contains invalid characters."))?;
    value.set_sensitive(true);
    let mut h = HeaderMap::new();
    h.insert("x-api-key", value);
    Ok(h)
}
fn http_error(status: u16) -> Error {
    bad(match status {401|403=>"Roblox rejected the key or its permissions. Check Settings, account access and IP restrictions.",429=>"Roblox rate limit reached. Wait before retrying.",400=>"Roblox rejected the upload. Check the texture and account upload requirements.",_=>"Roblox could not complete the request. Check your connection and try again."})
}
async fn body(mut r: Response) -> Result<Value, Error> {
    if !r.status().is_success() {
        return Err(http_error(r.status().as_u16()));
    }
    let mut data = Vec::new();
    while let Some(chunk) = r
        .chunk()
        .await
        .map_err(|_| bad("Roblox response was interrupted. Check status before retrying."))?
    {
        if data.len() + chunk.len() > 256 * 1024 {
            return Err(bad("Roblox returned an oversized response."));
        }
        data.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&data)
        .map_err(|_| bad("Roblox returned an unreadable response. Check status before retrying."))
}
fn scalar(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::to_owned)
        .or_else(|| v.as_u64().map(|n| n.to_string()))
}
fn validate_access(v: &Value, account: &str) -> Result<(), Error> {
    if v.get("enabled").and_then(Value::as_bool) != Some(true)
        || v.get("expired").and_then(Value::as_bool) != Some(false)
        || scalar(&v["authorizedUserId"]).as_deref() != Some(account)
    {
        return Err(bad(
            "Use an enabled, unexpired API key authorized by this Roblox account.",
        ));
    }
    for operation in ["read", "write"] {
        let permitted = v["scopes"].as_array().is_some_and(|scopes| {
            scopes.iter().any(|s| {
                s["name"] == "asset"
                    && s["operations"]
                        .as_array()
                        .is_some_and(|ops| ops.iter().any(|o| o == operation))
                    && s["userIds"].as_array().is_some_and(|ids| {
                        ids.iter()
                            .filter_map(scalar)
                            .any(|id| id == "*" || id == account)
                    })
            })
        });
        if !permitted {
            return Err(bad("Grant Assets read and write access for this personal account in the API key settings."));
        }
    }
    Ok(())
}
async fn check(c: &Credentials) -> Result<(), Error> {
    let response = client()?
        .post("https://apis.roblox.com/api-keys/v1/introspect")
        .json(&json!({"apiKey":c.api_key}))
        .send()
        .await
        .map_err(|_| bad("Cannot reach Roblox. The key was not changed."))?;
    validate_access(&body(response).await?, &c.account_id)
}
#[tauri::command]
pub async fn roblox_settings(
    state: tauri::State<'_, RobloxState>,
) -> Result<RobloxSettings, CommandError> {
    let _guard = state.0.lock().await;
    work(|| {
        Ok(match load()? {
            Some(c) => RobloxSettings {
                configured: true,
                account_id: Some(c.account_id.clone()),
            },
            None => RobloxSettings {
                configured: false,
                account_id: None,
            },
        })
    })
    .await
}
#[tauri::command]
pub async fn save_roblox_settings(
    account_id: String,
    api_key: String,
    state: tauri::State<'_, RobloxState>,
) -> Result<(), CommandError> {
    let _guard = state.0.lock().await;
    let c = Credentials {
        account_id: account_id.trim().into(),
        api_key: api_key.trim().into(),
    };
    let mut api_key = api_key;
    api_key.zeroize();
    if !numeric(&c.account_id) || c.api_key.is_empty() || c.api_key.len() > 2000 {
        return Err(bad("Enter a numeric account ID and an API key up to 2000 characters.").into());
    }
    check(&c).await?;
    work(move || {
        let bytes = Zeroizing::new(
            serde_json::to_vec(&c).map_err(|_| bad("Cannot encode Roblox settings."))?,
        );
        vault()?
            .set_secret(&bytes)
            .map_err(|_| bad("Windows could not securely save Roblox settings."))
    })
    .await
}
#[tauri::command]
pub async fn test_roblox_connection(
    state: tauri::State<'_, RobloxState>,
) -> Result<(), CommandError> {
    let _guard = state.0.lock().await;
    let c = work(required).await?;
    check(&c).await.map_err(Into::into)
}
#[tauri::command]
pub async fn clear_roblox_settings(
    state: tauri::State<'_, RobloxState>,
) -> Result<(), CommandError> {
    let _guard = state.0.lock().await;
    work(|| match vault()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(bad("Windows could not remove Roblox settings.")),
    })
    .await
}
#[tauri::command]
pub async fn roblox_livery(
    folder_id: String,
    workspace: tauri::State<'_, Workspace>,
    state: tauri::State<'_, RobloxState>,
) -> Result<LiveryUploads, CommandError> {
    let _guard = state.0.lock().await;
    let ws = workspace.inner().clone();
    work(move || {
        let account = load()?.map(|c| c.account_id.clone()).unwrap_or_default();
        ws.livery_uploads(&folder_id, &account)
    })
    .await
}
fn apply_operation(upload: &mut DecalUpload, v: &Value) -> Result<(), Error> {
    if v.get("error").is_some_and(|e| !e.is_null()) {
        upload.state = "failed".into();
        upload.operation = None;
        upload.message =
            "Roblox could not process this texture. Check it in Creator Dashboard.".into();
        return Ok(());
    }
    if v["done"] != true {
        upload.state = "processing".into();
        return Ok(());
    }
    let asset = &v["response"];
    if !["Decal", "ASSET_TYPE_DECAL"].contains(&asset["assetType"].as_str().unwrap_or("")) {
        return Err(bad(
            "Roblox did not return a Decal asset. No image ID was accepted.",
        ));
    }
    let id = scalar(&asset["assetId"])
        .filter(|s| numeric(s))
        .ok_or_else(|| bad("Roblox did not return a decal asset ID."))?;
    if scalar(&asset["creationContext"]["creator"]["userId"]).as_deref() != Some(&upload.account_id)
    {
        return Err(bad(
            "Roblox returned a different asset owner. Check account settings.",
        ));
    }
    upload.asset_id = Some(id);
    upload.state = "uploaded".into();
    upload.moderation = match asset["moderationResult"]["moderationState"]
        .as_str()
        .unwrap_or("")
    {
        "Approved" | "MODERATION_STATE_APPROVED" => "approved",
        "Reviewing" | "MODERATION_STATE_REVIEWING" => "reviewing",
        "Rejected" | "MODERATION_STATE_REJECTED" => "rejected",
        _ => "unknown",
    }
    .into();
    upload.message.clear();
    Ok(())
}
fn operation_path(v: &Value) -> Result<String, Error> {
    let path = v["path"].as_str().unwrap_or("");
    let id = path
        .strip_prefix("operations/")
        .ok_or_else(|| bad("Roblox did not return an upload operation."))?;
    if id.is_empty()
        || id.len() > 150
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(bad("Roblox returned an invalid operation reference."));
    }
    Ok(path.into())
}
async fn create_decal(
    http: &Client,
    url: &str,
    c: &Credentials,
    display_name: &str,
    bytes: Vec<u8>,
) -> Result<Response, Error> {
    let request = json!({"assetType":"Decal","displayName":display_name,"description":"","creationContext":{"creator":{"userId":c.account_id},"expectedPrice":0}});
    let form = reqwest::multipart::Form::new()
        .text("request", request.to_string())
        .part(
            "fileContent",
            reqwest::multipart::Part::bytes(bytes)
                .file_name("texture.png")
                .mime_str("image/png")
                .map_err(|_| bad("Invalid PNG upload."))?,
        );
    http.post(url)
        .headers(headers(c)?)
        .multipart(form)
        .send()
        .await
        .map_err(|_| {
            bad("Upload response was lost. Check Creator Dashboard before creating another decal.")
        })
}
#[tauri::command]
pub async fn upload_roblox_texture(
    folder_id: String,
    hash: String,
    workspace: tauri::State<'_, Workspace>,
    state: tauri::State<'_, RobloxState>,
) -> Result<DecalUpload, CommandError> {
    let _guard = state.0.lock().await;
    let c = work(required).await?;
    let ws = workspace.inner().clone();
    let ws2 = ws.clone();
    let account = c.account_id.clone();
    let h = hash.clone();
    let f = folder_id.clone();
    let (claimed, texture, bytes) = work(move || {
        let livery = ws2.livery_uploads(&f, &account)?;
        let texture = livery
            .textures
            .into_iter()
            .find(|t| t.hash == h)
            .ok_or(Error::NotFound)?;
        let bytes = ws2.decal_bytes(&h)?;
        let claimed = ws2.claim_decal(&f, &account, &h)?;
        Ok((claimed, texture, bytes))
    })
    .await?;
    let mut upload = ws
        .decal_upload(&c.account_id, &hash)?
        .ok_or(Error::NotFound)?;
    let http = client()?;
    if claimed {
        let display_name = format!("{} — {}", ws.folder(&folder_id)?.name, texture.side)
            .chars()
            .take(50)
            .collect::<String>();
        let sent = create_decal(
            &http,
            "https://apis.roblox.com/assets/v1/assets",
            &c,
            &display_name,
            bytes,
        )
        .await;
        let result = match sent {
            Ok(response) => {
                let status = response.status();
                if !status.is_success() {
                    upload.state = if status.is_client_error() && status.as_u16() != 408 {
                        "failed"
                    } else {
                        "uncertain"
                    }
                    .into();
                    Err(http_error(status.as_u16()))
                } else {
                    match body(response).await {
                        Ok(v) => operation_path(&v).map(|p| {
                            upload.operation = Some(p);
                            upload.state = "processing".into();
                        }),
                        Err(e) => Err(e),
                    }
                }
            }
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            if upload.state == "uploading" {
                upload.state = "uncertain".into();
            }
            upload.message = e.to_string();
            ws.save_decal_upload(&upload)?;
            return Ok(upload);
        }
        ws.save_decal_upload(&upload)?;
    }
    if let Some(id) = upload.asset_id.clone() {
        if !numeric(&id) {
            return Err(bad("Invalid saved decal asset ID.").into());
        }
        let response = http
            .get(format!("https://apis.roblox.com/assets/v1/assets/{id}"))
            .headers(headers(&c)?)
            .send()
            .await;
        match response {
            Ok(r) => match body(r).await {
                Ok(asset) => {
                    if let Err(e) =
                        apply_operation(&mut upload, &json!({"done":true,"response":asset}))
                    {
                        upload.message = e.to_string();
                    }
                }
                Err(e) => upload.message = e.to_string(),
            },
            Err(_) => {
                upload.message = "Cannot refresh moderation. The saved decal ID is retained.".into()
            }
        }
        ws.save_decal_upload(&upload)?;
        return Ok(upload);
    }
    if let Some(path) = upload.operation.clone() {
        operation_path(&json!({"path":path}))?;
        for attempt in 0..5 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            let result = async {
                let r = http
                    .get(format!("https://apis.roblox.com/assets/v1/{path}"))
                    .headers(headers(&c)?)
                    .send()
                    .await
                    .map_err(|_| {
                        bad("Cannot check Roblox yet. Use Check status to resume this operation.")
                    })?;
                apply_operation(&mut upload, &body(r).await?)
            }
            .await;
            if let Err(e) = result {
                upload.message = e.to_string();
                break;
            }
            if upload.state != "processing" {
                break;
            }
        }
        ws.save_decal_upload(&upload)?;
    } else if !claimed && upload.state == "uploading" {
        upload.state = "uncertain".into();
        upload.message="An earlier upload was interrupted. Check Creator Dashboard and enter its decal ID manually.".into();
        ws.save_decal_upload(&upload)?;
    }
    Ok(upload)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn multipart_upload_sends_decal_type_and_owner_then_reads_operation() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/assets/v1/assets", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut data = Vec::new();
            let mut chunk = [0; 4096];
            loop {
                let n = socket.read(&mut chunk).unwrap();
                assert!(n > 0);
                data.extend_from_slice(&chunk[..n]);
                if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&data[..end]);
                    let length = header
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|n| n.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if data.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8_lossy(&data);
            assert!(request.contains("\"assetType\":\"Decal\""));
            assert!(request.contains("\"userId\":\"123\""));
            assert!(request.contains("name=\"fileContent\""));
            assert!(request.contains("image/png"));
            assert!(!request.contains("\"assetType\":\"Image\""));
            let response = r#"{"path":"operations/test-operation"}"#;
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        });
        let c = Credentials {
            account_id: "123".into(),
            api_key: "dummy-key".into(),
        };
        let operation = tauri::async_runtime::block_on(async {
            let response = create_decal(&Client::new(), &url, &c, "Patrol Left 1", vec![1, 2, 3])
                .await
                .unwrap();
            operation_path(&body(response).await.unwrap()).unwrap()
        });
        server.join().unwrap();
        assert_eq!(operation, "operations/test-operation");
    }
    #[test]
    fn key_access_requires_both_scopes_and_matching_personal_account() {
        let mut v = json!({"authorizedUserId":123,"enabled":true,"expired":false,"scopes":[{"name":"asset","operations":["read","write"],"userIds":["123"]}]});
        assert!(validate_access(&v, "123").is_ok());
        assert!(validate_access(&v, "456").is_err());
        v["scopes"][0]["operations"] = json!(["read"]);
        assert!(validate_access(&v, "123").is_err());
    }
    #[test]
    fn operation_accepts_only_owned_decal_asset_id_and_tracks_moderation() {
        let mut u = DecalUpload {
            account_id: "123".into(),
            hash: "a".repeat(64),
            state: "processing".into(),
            operation: Some("operations/test".into()),
            asset_id: None,
            moderation: "unknown".into(),
            message: String::new(),
        };
        let mut v = json!({"done":true,"response":{"assetType":"ASSET_TYPE_DECAL","assetId":"456","creationContext":{"creator":{"userId":123}},"moderationResult":{"moderationState":"Reviewing"}}});
        apply_operation(&mut u, &v).unwrap();
        assert_eq!(u.asset_id.as_deref(), Some("456"));
        assert_eq!(u.moderation, "reviewing");
        v["response"]["assetType"] = json!("Image");
        assert!(apply_operation(&mut u, &v).is_err());
        assert!(operation_path(&json!({"path":"operations/../../evil"})).is_err());
    }
    #[test]
    #[cfg(target_os = "windows")]
    fn windows_vault_round_trips_a_disposable_nonsecret_record() {
        let name = format!("apex-test-{}", uuid::Uuid::now_v7());
        let e = keyring::Entry::new(&name, "test").unwrap();
        e.set_secret(br#"{"accountId":"123","apiKey":"not-a-real-key"}"#)
            .unwrap();
        let read = e.get_secret().unwrap();
        e.delete_credential().unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&read).unwrap()["accountId"],
            "123"
        );
    }
}
