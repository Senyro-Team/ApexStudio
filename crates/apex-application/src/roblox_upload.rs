//! Roblox decal upload transport shared by the desktop adapter and CLI.
//!
//! Queue scheduling remains an adapter concern for now; this module owns the
//! HTTP and response-validation rules so both adapters submit Decals and accept
//! only owned Decal asset IDs in the same way.

use crate::RobloxCredentials;
use apex_core::Error;
use apex_storage::DecalUpload;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Response,
};
use serde_json::{json, Value};
use std::time::Duration;

fn bad(message: &str) -> Error {
    Error::InvalidContent(message.into())
}

/// Construct the HTTPS-only HTTP client used for Roblox asset uploads.
pub fn client() -> Result<Client, Error> {
    Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|_| bad("Could not start the Roblox connection."))
}

pub fn headers(credentials: &RobloxCredentials) -> Result<HeaderMap, Error> {
    let mut value = HeaderValue::from_str(&credentials.api_key)
        .map_err(|_| bad("The API key contains invalid characters."))?;
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", value);
    Ok(headers)
}

fn numeric(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn scalar(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|number| number.to_string()))
}

/// Read a bounded Roblox JSON response body.
pub async fn body(mut response: Response) -> Result<Value, Error> {
    if !response.status().is_success() {
        let message = match response.status().as_u16() {
            401 | 403 => "Roblox rejected the key or its permissions. Check account access, API-key scopes and IP restrictions.",
            429 => "Roblox rate limit reached. Wait before retrying.",
            400 => "Roblox rejected the upload. Check the texture and account upload requirements.",
            _ => "Roblox could not complete the request. Check your connection and try again.",
        };
        return Err(bad(message));
    }
    let mut data = Vec::new();
    while let Some(chunk) = response
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

/// Validate that an operation reference has the shape returned by Roblox.
pub fn operation_path(value: &Value) -> Result<String, Error> {
    let path = value["path"].as_str().unwrap_or("");
    let id = path
        .strip_prefix("operations/")
        .ok_or_else(|| bad("Roblox did not return an upload operation."))?;
    if id.is_empty()
        || id.len() > 150
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(bad("Roblox returned an invalid operation reference."));
    }
    Ok(path.into())
}

/// Upload PNG bytes as a Roblox Decal, never as an Image asset.
pub async fn create_decal(
    http: &Client,
    url: &str,
    credentials: &RobloxCredentials,
    display_name: &str,
    bytes: Vec<u8>,
) -> Result<Response, Error> {
    let request = json!({
        "assetType": "Decal",
        "displayName": display_name,
        "description": "",
        "creationContext": {
            "creator": { "userId": credentials.account_id },
            "expectedPrice": 0
        }
    });
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
        .headers(headers(credentials)?)
        .multipart(form)
        .send()
        .await
        .map_err(|_| {
            bad("Upload response was lost. Check Creator Dashboard before creating another decal.")
        })
}

/// Apply a completed Roblox operation only when it returned an owned Decal ID.
pub fn apply_operation(upload: &mut DecalUpload, value: &Value) -> Result<(), Error> {
    if value.get("error").is_some_and(|error| !error.is_null()) {
        upload.state = "failed".into();
        upload.operation = None;
        upload.message =
            "Roblox could not process this texture. Check it in Creator Dashboard.".into();
        return Ok(());
    }
    if value["done"] != true {
        upload.state = "processing".into();
        return Ok(());
    }
    let asset = &value["response"];
    if !["Decal", "ASSET_TYPE_DECAL"].contains(&asset["assetType"].as_str().unwrap_or("")) {
        return Err(bad(
            "Roblox did not return a Decal asset. No image ID was accepted.",
        ));
    }
    let id = scalar(&asset["assetId"])
        .filter(|id| numeric(id))
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn upload() -> DecalUpload {
        DecalUpload {
            account_id: "123".into(),
            hash: "a".repeat(64),
            state: "processing".into(),
            operation: Some("operations/example".into()),
            asset_id: None,
            moderation: "unknown".into(),
            message: String::new(),
        }
    }

    #[test]
    fn operation_path_accepts_only_a_simple_operation_identifier() {
        assert_eq!(
            operation_path(&json!({"path":"operations/job-1_A"})).expect("valid operation"),
            "operations/job-1_A"
        );
        for path in [
            "",
            "operations/",
            "operations/../other",
            "https://example.com",
        ] {
            assert!(operation_path(&json!({"path":path})).is_err(), "{path}");
        }
    }

    #[test]
    fn completed_asset_must_be_a_decal_owned_by_the_configured_account() {
        let response = json!({
            "done": true,
            "response": {
                "assetType": "ASSET_TYPE_DECAL",
                "assetId": "456",
                "creationContext": {"creator": {"userId": 123}},
                "moderationResult": {"moderationState": "Reviewing"}
            }
        });
        let mut result = upload();
        apply_operation(&mut result, &response).expect("owned decal");
        assert_eq!(result.asset_id.as_deref(), Some("456"));
        assert_eq!(result.state, "uploaded");
        assert_eq!(result.moderation, "reviewing");

        let mut result = upload();
        let mut image = response.clone();
        image["response"]["assetType"] = json!("Image");
        assert!(apply_operation(&mut result, &image).is_err());

        let mut other_owner = response;
        other_owner["response"]["creationContext"]["creator"]["userId"] = json!(987);
        assert!(apply_operation(&mut result, &other_owner).is_err());
    }

    #[test]
    fn multipart_transport_submits_png_bytes_as_a_decal_for_the_configured_owner() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("mock listener");
        let url = format!(
            "http://{}/assets/v1/assets",
            listener.local_addr().expect("mock address")
        );
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("mock request");
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            let mut data = Vec::new();
            let mut chunk = [0; 4096];
            loop {
                let count = socket.read(&mut chunk).expect("read request");
                assert!(count > 0);
                data.extend_from_slice(&chunk[..count]);
                if let Some(end) = data.windows(4).position(|window| window == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&data[..end]);
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .expect("content length");
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
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )
            .expect("mock response");
        });

        let credentials = RobloxCredentials {
            account_id: "123".into(),
            api_key: "dummy-key".to_owned().into(),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let operation = runtime.block_on(async {
            let response = create_decal(
                &Client::new(),
                &url,
                &credentials,
                "Patrol Left 1",
                vec![1, 2, 3],
            )
            .await
            .expect("decal request");
            operation_path(&body(response).await.expect("operation response"))
                .expect("operation path")
        });
        server.join().expect("mock server");
        assert_eq!(operation, "operations/test-operation");
    }
}
