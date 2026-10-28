#[allow(dead_code)] // Shared protocol also provides the app-side host name.
mod chrome_protocol;
use std::io;
fn main() {
    if run().is_err() {
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
    let config = chrome_protocol::config()?;
    let origin = std::env::args()
        .nth(1)
        .ok_or_else(|| io::Error::other("Missing Chrome origin."))?;
    if !chrome_protocol::valid_extension(&config.extension_id)
        || origin != format!("chrome-extension://{}/", config.extension_id)
    {
        return Err(io::Error::other("Unapproved extension."));
    }
    if !config.pipe.starts_with(r"\\.\pipe\ApexStudio-") || config.pipe.len() > 160 {
        return Err(io::Error::other("Invalid app endpoint."));
    }
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
            return Err(io::Error::other("Invalid request."));
        }
        request["origin"] = origin.clone().into();
        let response = runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(15), async {
                let mut pipe = ClientOptions::new().open(&config.pipe)?;
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
        chrome_protocol::write_frame(&mut output, &response)?;
    }
}
#[cfg(not(windows))]
fn run() -> io::Result<()> {
    Err(io::Error::other("Chrome bridge requires Windows."))
}
