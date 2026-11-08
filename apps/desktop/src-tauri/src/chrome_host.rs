#[allow(dead_code)] // Shared protocol also provides the app-side host name.
mod chrome_protocol;
use std::io;
fn diagnostic(event: &str) {
    use std::{
        io::Write,
        time::{SystemTime, UNIX_EPOCH},
    };
    let Ok(directory) = chrome_protocol::directory() else {
        return;
    };
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("bridge-diagnostics.log"))
    else {
        return;
    };
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let _ = writeln!(file, "{millis} {event}");
}
fn main() {
    if run().is_err() {
        diagnostic("host_exit_error");
        // Native stdout is reserved exclusively for framed responses.
        let _=chrome_protocol::write_frame(&mut io::stdout().lock(),br#"{"error":"Cannot connect to Apex Studio. Open the app and check Chrome settings."}"#);
    }
}
#[cfg(windows)]
fn run() -> io::Result<()> {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::windows::named_pipe::ClientOptions,
    };
    diagnostic("host_started");
    let config = chrome_protocol::config().map_err(|error| {
        diagnostic("settings_read_failed");
        error
    })?;
    let origin = std::env::args()
        .nth(1)
        .ok_or_else(|| io::Error::other("Missing Chrome origin."))?;
    if !chrome_protocol::valid_extension(&config.extension_id)
        || origin != format!("chrome-extension://{}/", config.extension_id)
    {
        diagnostic("extension_origin_rejected");
        return Err(io::Error::other("Unapproved extension."));
    }
    if !config.pipe.starts_with(r"\\.\pipe\ApexStudio-") || config.pipe.len() > 160 {
        diagnostic("app_pipe_config_invalid");
        return Err(io::Error::other("Invalid app endpoint."));
    }
    diagnostic("host_ready");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    loop {
        let bytes = match chrome_protocol::read_frame(&mut input, 16 * 1024) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let mut request: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if !request.is_object() {
            diagnostic("request_invalid");
            return Err(io::Error::other("Invalid request."));
        }
        if request["kind"].as_str() == Some("check") {
            diagnostic("connection_check_received");
        } else {
            diagnostic("request_received");
        }
        request["origin"] = origin.clone().into();
        let response = runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(15), async {
                let mut pipe = ClientOptions::new().open(&config.pipe).map_err(|error| {
                    diagnostic("app_pipe_connect_failed");
                    error
                })?;
                let payload = serde_json::to_vec(&request).map_err(io::Error::other)?;
                pipe.write_u32_le(payload.len() as u32).await?;
                pipe.write_all(&payload).await?;
                let size = pipe.read_u32_le().await? as usize;
                if size > chrome_protocol::MAX_FRAME {
                    return Err(io::Error::other("Invalid response."));
                }
                let mut bytes = vec![0; size];
                pipe.read_exact(&mut bytes).await?;
                Ok::<_, io::Error>(bytes)
            })
            .await
            .map_err(io::Error::other)?
        })?;
        let parsed: serde_json::Value = serde_json::from_slice(&response)
            .unwrap_or_else(|_| serde_json::json!({"error":"invalid_response"}));
        if parsed.get("error").is_some() {
            diagnostic("app_rejected_request");
        } else {
            diagnostic("app_responded");
        }
        chrome_protocol::write_frame(&mut output, &response)?;
    }
}
#[cfg(not(windows))]
fn run() -> io::Result<()> {
    Err(io::Error::other("Chrome bridge requires Windows."))
}
