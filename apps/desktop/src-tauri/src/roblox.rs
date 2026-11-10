use super::{projects::work, CommandError};
use apex_core::Error;
use apex_storage::{DecalUpload, LiveryUploads, Workspace};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Response,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn save_livery_codes_markdown(
    markdown: String,
    app: tauri::AppHandle,
) -> Result<bool, CommandError> {
    work(move || {
        if markdown.is_empty() || markdown.len() > 5_000_000 {
            return Err(bad(
                "The Livery codes preview is empty or too large to save.",
            ));
        }
        let Some(file) = app
            .dialog()
            .file()
            .set_title("Save Livery codes")
            .add_filter("Markdown", &["md"])
            .set_file_name("livery-codes.md")
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = file
            .into_path()
            .map_err(|e| Error::Storage(e.to_string()))?;
        std::fs::write(path, markdown).map_err(|e| Error::Storage(e.to_string()))?;
        Ok(true)
    })
    .await
}

#[derive(Default)]
pub struct RobloxState(pub tokio::sync::Mutex<()>);
pub(super) use apex_application::RobloxCredentials as Credentials;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RobloxSettings {
    configured: bool,
    account_id: Option<String>,
    double_upload: bool,
}
fn bad(message: &str) -> Error {
    Error::InvalidContent(message.into())
}
pub(super) fn load() -> Result<Option<Credentials>, Error> {
    apex_application::load_roblox_credentials()
}
pub(super) fn required() -> Result<Credentials, Error> {
    load()?.ok_or_else(|| bad("Configure Roblox in Settings before uploading."))
}
pub(super) fn numeric(s: &str) -> bool {
    !s.is_empty() && s.len() <= 20 && !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit())
}
pub(super) fn client() -> Result<Client, Error> {
    Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|_| bad("Could not start the Roblox connection."))
}
pub(super) fn headers(c: &Credentials) -> Result<HeaderMap, Error> {
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
fn scalar(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::to_owned)
        .or_else(|| v.as_u64().map(|n| n.to_string()))
}
pub(super) async fn body(mut r: Response) -> Result<Value, Error> {
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
#[tauri::command]
pub async fn roblox_settings(
    state: tauri::State<'_, RobloxState>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<RobloxSettings, CommandError> {
    let _guard = state.0.lock().await;
    let ws = workspace.inner().clone();
    work(move || {
        let (configured, account_id) = match load()? {
            Some(c) => (true, Some(c.account_id.clone())),
            None => (false, None),
        };
        Ok(RobloxSettings {
            configured,
            account_id,
            double_upload: ws.roblox_double_upload()?,
        })
    })
    .await
}
#[tauri::command]
pub async fn set_roblox_double_upload(
    enabled: bool,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.set_roblox_double_upload(enabled)).await
}
#[tauri::command]
pub async fn save_roblox_settings(
    account_id: String,
    api_key: String,
    workspace: tauri::State<'_, Workspace>,
    runtime: tauri::State<'_, std::sync::Arc<super::upload_queue::UploadRuntime>>,
    state: tauri::State<'_, RobloxState>,
) -> Result<(), CommandError> {
    let _change = runtime.stop_for_credentials(workspace.inner()).await;
    let _guard = state.0.lock().await;
    apex_application::save_roblox_credentials(account_id, api_key)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub async fn test_roblox_connection(
    state: tauri::State<'_, RobloxState>,
) -> Result<(), CommandError> {
    let _guard = state.0.lock().await;
    let c = work(required).await?;
    apex_application::validate_roblox_credentials(&c)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub async fn clear_roblox_settings(
    workspace: tauri::State<'_, Workspace>,
    runtime: tauri::State<'_, std::sync::Arc<super::upload_queue::UploadRuntime>>,
    state: tauri::State<'_, RobloxState>,
) -> Result<(), CommandError> {
    let _change = runtime.stop_for_credentials(workspace.inner()).await;
    let _guard = state.0.lock().await;
    work(apex_application::clear_roblox_credentials).await
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
pub(super) fn apply_operation(upload: &mut DecalUpload, v: &Value) -> Result<(), Error> {
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
pub(super) fn operation_path(v: &Value) -> Result<String, Error> {
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
pub(super) async fn create_decal(
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
            api_key: "dummy-key".to_owned().into(),
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
