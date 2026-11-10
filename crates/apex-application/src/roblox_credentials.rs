use apex_core::Error;
use reqwest::{redirect::Policy, Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use zeroize::{Zeroize, Zeroizing};

const SERVICE: &str = "studio.apexdesigns.apexstudio";
const USER: &str = "roblox-personal";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CredentialRecord {
    account_id: String,
    api_key: String,
}

impl Drop for CredentialRecord {
    fn drop(&mut self) {
        self.account_id.zeroize();
        self.api_key.zeroize();
    }
}

/// Roblox API credentials held only in memory after reading from the OS vault.
pub struct RobloxCredentials {
    pub account_id: String,
    pub api_key: Zeroizing<String>,
}

impl Drop for RobloxCredentials {
    fn drop(&mut self) {
        self.account_id.zeroize();
    }
}

/// Credentials which passed Roblox's owner, expiry, and scope checks.
pub struct ValidatedRobloxCredentials(RobloxCredentials);

fn bad(message: &str) -> Error {
    Error::InvalidContent(message.into())
}

#[cfg(windows)]
fn entry() -> Result<keyring::Entry, Error> {
    keyring::Entry::new(SERVICE, USER)
        .map_err(|_| bad("Windows Credential Manager is unavailable."))
}

#[cfg(not(windows))]
fn entry() -> Result<(), Error> {
    let _ = (SERVICE, USER);
    Err(bad(
        "Secure Roblox settings are currently available on Windows.",
    ))
}

/// Read the personal Roblox credentials shared by the desktop app and CLI.
pub fn load_roblox_credentials() -> Result<Option<RobloxCredentials>, Error> {
    #[cfg(windows)]
    {
        match entry()?.get_secret() {
            Ok(bytes) => {
                let bytes = Zeroizing::new(bytes);
                let record: CredentialRecord = serde_json::from_slice(&bytes).map_err(|_| {
                    bad("Saved Roblox credentials cannot be read. Replace them in Settings.")
                })?;
                Ok(Some(RobloxCredentials {
                    account_id: record.account_id.clone(),
                    api_key: Zeroizing::new(record.api_key.clone()),
                }))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(bad("Windows could not unlock Roblox credentials.")),
        }
    }
    #[cfg(not(windows))]
    {
        entry()?;
        Ok(None)
    }
}

fn valid_account(account_id: &str) -> bool {
    !account_id.is_empty()
        && account_id.len() <= 20
        && !account_id.starts_with('0')
        && account_id.bytes().all(|byte| byte.is_ascii_digit())
}

fn validate_access(value: &Value, account_id: &str) -> Result<(), Error> {
    let authorized = value["authorizedUserId"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| value["authorizedUserId"].as_u64().map(|id| id.to_string()));
    if value["enabled"].as_bool() != Some(true)
        || value["expired"].as_bool() != Some(false)
        || authorized.as_deref() != Some(account_id)
    {
        return Err(bad(
            "Use an enabled, unexpired API key authorized by this Roblox account.",
        ));
    }
    for operation in ["read", "write"] {
        let permitted = value["scopes"].as_array().is_some_and(|scopes| {
            scopes.iter().any(|scope| {
                scope["name"] == "asset"
                    && scope["operations"]
                        .as_array()
                        .is_some_and(|operations| operations.iter().any(|item| item == operation))
                    && scope["userIds"].as_array().is_some_and(|user_ids| {
                        user_ids.iter().any(|id| {
                            id == "*"
                                || id.as_str() == Some(account_id)
                                || id.as_u64().is_some_and(|id| id.to_string() == account_id)
                        })
                    })
            })
        });
        if !permitted {
            return Err(bad(
                "Grant Assets read and write access for this personal account in the API key settings.",
            ));
        }
    }
    Ok(())
}

async fn response_json(mut response: Response) -> Result<Value, Error> {
    if !response.status().is_success() {
        let message = match response.status().as_u16() {
            401 | 403 => "Roblox rejected the key or its permissions.",
            429 => "Roblox rate limit reached. Wait before retrying.",
            _ => "Roblox could not validate this API key.",
        };
        return Err(bad(message));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| bad("Roblox interrupted the API-key validation response."))?
    {
        if bytes.len() + chunk.len() > 256 * 1024 {
            return Err(bad("Roblox returned an oversized validation response."));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| bad("Roblox returned an unreadable response."))
}

/// Validate key owner, expiry, and asset read/write scopes through Roblox's key introspection API.
pub async fn validate_roblox_credentials(credentials: &RobloxCredentials) -> Result<(), Error> {
    if !valid_account(&credentials.account_id)
        || credentials.api_key.is_empty()
        || credentials.api_key.len() > 2000
    {
        return Err(bad(
            "Enter a numeric account ID and an API key up to 2000 characters.",
        ));
    }
    let client = Client::builder()
        .https_only(true)
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| bad("Could not start the Roblox connection."))?;
    introspect_at(
        credentials,
        &client,
        "https://apis.roblox.com/api-keys/v1/introspect",
    )
    .await
}

async fn introspect_at(
    credentials: &RobloxCredentials,
    client: &Client,
    endpoint: &str,
) -> Result<(), Error> {
    let response = client
        .post(endpoint)
        .json(&serde_json::json!({"apiKey": credentials.api_key.as_str()}))
        .send()
        .await
        .map_err(|_| bad("Cannot reach Roblox. The saved key was not changed."))?;
    validate_access(&response_json(response).await?, &credentials.account_id)
}

/// Validate new credentials before replacing the desktop app's existing vault entry.
pub async fn prepare_roblox_credentials(
    account_id: String,
    api_key: String,
) -> Result<ValidatedRobloxCredentials, Error> {
    let account_id = account_id.trim().to_owned();
    let api_key = Zeroizing::new(api_key);
    let api_key = Zeroizing::new(api_key.trim().to_owned());
    let credentials = RobloxCredentials {
        account_id,
        api_key: api_key.clone(),
    };
    validate_roblox_credentials(&credentials).await?;

    Ok(ValidatedRobloxCredentials(credentials))
}

/// Store credentials returned by [`prepare_roblox_credentials`] in the OS vault.
pub fn store_roblox_credentials(credentials: ValidatedRobloxCredentials) -> Result<(), Error> {
    let record = CredentialRecord {
        account_id: credentials.0.account_id.clone(),
        api_key: credentials.0.api_key.to_string(),
    };
    let bytes = Zeroizing::new(
        serde_json::to_vec(&record).map_err(|_| bad("Cannot encode Roblox settings."))?,
    );
    #[cfg(windows)]
    entry()?
        .set_secret(&bytes)
        .map_err(|_| bad("Windows could not securely save Roblox settings."))?;
    #[cfg(not(windows))]
    {
        let _ = bytes;
        entry()?;
    }
    Ok(())
}

/// Validate new credentials and replace the shared OS-vault entry only on success.
pub async fn save_roblox_credentials(account_id: String, api_key: String) -> Result<(), Error> {
    let credentials = prepare_roblox_credentials(account_id, api_key).await?;
    store_roblox_credentials(credentials)
}

/// Remove the shared personal Roblox credentials from the OS vault.
pub fn clear_roblox_credentials() -> Result<(), Error> {
    #[cfg(windows)]
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(bad("Windows could not remove Roblox settings.")),
    }
    #[cfg(not(windows))]
    {
        entry()?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credential_key_must_be_enabled_unexpired_and_scoped_to_the_selected_account() {
        let mut value = json!({
            "authorizedUserId": 123,
            "enabled": true,
            "expired": false,
            "scopes": [{"name":"asset", "operations":["read", "write"], "userIds":["123"]}]
        });
        assert!(validate_access(&value, "123").is_ok());
        assert!(validate_access(&value, "456").is_err());
        value["scopes"][0]["operations"] = json!(["read"]);
        assert!(validate_access(&value, "123").is_err());
        value["scopes"][0]["operations"] = json!(["read", "write"]);
        value["expired"] = json!(true);
        assert!(validate_access(&value, "123").is_err());
    }

    #[test]
    fn credential_introspection_uses_the_expected_request_and_accepts_a_valid_mock_response() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("mock listener");
        let endpoint = format!(
            "http://{}/introspect",
            listener.local_addr().expect("mock address")
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("mock connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            let mut request = Vec::new();
            let mut chunk = [0; 1024];
            loop {
                let count = stream.read(&mut chunk).expect("read request");
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..headers_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .expect("request content length");
                if request.len() >= headers_end + 4 + content_length {
                    break;
                }
            }
            let body = r#"{"authorizedUserId":123,"enabled":true,"expired":false,"scopes":[{"name":"asset","operations":["read","write"],"userIds":["123"]}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write mock response");
        });
        let credentials = RobloxCredentials {
            account_id: "123".into(),
            api_key: Zeroizing::new("not-a-real-key".into()),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        runtime
            .block_on(introspect_at(&credentials, &Client::new(), &endpoint))
            .expect("valid mock credentials");
        server.join().expect("mock server");
    }
}
