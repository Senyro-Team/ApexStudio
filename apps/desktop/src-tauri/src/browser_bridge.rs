use super::{chrome_protocol as protocol, projects::work, CommandError};
use apex_core::Error;
use apex_storage::{RobloxGroupSale, RobloxRevenuePass, SalesData, Workspace};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::Manager;
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub connected: bool,
    pub state: String,
    pub step: String,
    pub message: String,
    pub post_id: Option<String>,
}
struct Transfer {
    token: String,
    session: Option<String>,
    expires: Instant,
    bundle: tempfile::NamedTempFile,
    offset: usize,
    size: usize,
    post_id: String,
    command: String,
}
struct StatisticsTransfer {
    token: String,
    session: Option<String>,
    profile_id: String,
    expires: Instant,
    state: String,
    message: String,
}
struct GroupRevenueTransfer {
    token: String,
    session: Option<String>,
    group_id: String,
    expires: Instant,
    state: String,
    message: String,
    passes: Vec<RobloxRevenuePass>,
    sales: Vec<RobloxGroupSale>,
    coverage_from: Option<String>,
    coverage_to: Option<String>,
    summary: Option<Value>,
}
#[derive(Default)]
struct BridgeState {
    config: protocol::ChromeConfig,
    check: Option<(String, Instant)>,
    checked: Option<Instant>,
    transfer: Option<Transfer>,
    pending: Option<String>,
    status: BrowserStatus,
    statistics: Option<StatisticsTransfer>,
    group_revenue: Option<GroupRevenueTransfer>,
    workspace: Option<Workspace>,
}
#[derive(Clone, Default)]
pub struct BrowserBridge(Arc<Mutex<BridgeState>>);
fn invalid(message: &str) -> Error {
    Error::InvalidContent(message.into())
}
fn secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::now_v7().simple(),
        uuid::Uuid::now_v7().simple()
    )
}
impl BrowserBridge {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, BridgeState>, Error> {
        self.0
            .lock()
            .map_err(|_| invalid("Browser connection is unavailable."))
    }
    pub(super) fn invalidate(&self) -> Result<(), Error> {
        let mut s = self.lock()?;
        s.transfer = None;
        s.statistics = None;
        s.group_revenue = None;
        s.pending = None;
        s.checked = None;
        s.check = None;
        s.status.state = "cancelled".into();
        s.status.message = "Workspace changed. Start a new browser handoff.".into();
        Ok(())
    }
    fn snapshot(&self) -> Result<BrowserStatus, Error> {
        let mut state = self.lock()?;
        if state
            .check
            .as_ref()
            .is_some_and(|(_, deadline)| Instant::now() > *deadline)
        {
            state.check = None;
            state.status = BrowserStatus {
                connected: false,
                state: "failed".into(),
                step: "Connection check".into(),
                message: "Chrome did not confirm the connection. Check that the Apex Studio extension is enabled in this browser profile, then try again.".into(),
                post_id: None,
            };
        }
        if state
            .transfer
            .as_ref()
            .is_some_and(|t| Instant::now() > t.expires)
        {
            state.transfer = None;
            state.status.state = "expired".into();
            state.status.message = "Browser handoff expired. Start it again.".into();
        }
        let mut out = state.status.clone();
        out.connected = state
            .checked
            .is_some_and(|t| t.elapsed() < Duration::from_secs(300));
        Ok(out)
    }
    fn respond(&self, request: Value) -> Value {
        match self.process(request) {
            Ok(v) => v,
            Err(e) => {
                json!({"error":e.to_string(),"cancelled":self.lock().is_ok_and(|s|s.status.state=="cancelled")})
            }
        }
    }
    fn process(&self, r: Value) -> Result<Value, Error> {
        let mut s = self.lock()?;
        if r["origin"].as_str()
            != Some(format!("chrome-extension://{}/", s.config.extension_id).as_str())
        {
            return Err(invalid("Unapproved extension."));
        }
        let kind = r["kind"].as_str().unwrap_or("");
        if kind == "check" {
            let Some((token, deadline)) = s.check.take() else {
                if s.status.state == "checking" {
                    s.status.state = "failed".into();
                    s.status.step = "Connection check".into();
                    s.status.message =
                        "The browser connection check expired. Try again from Settings.".into();
                }
                return Err(invalid("Connection check expired."));
            };
            if r["token"].as_str() != Some(&token) || Instant::now() > deadline {
                s.status.state = "failed".into();
                s.status.step = "Connection check".into();
                s.status.message = "The browser returned an invalid or expired connection check. Try again from Settings.".into();
                return Err(invalid("Connection check expired."));
            }
            s.checked = Some(Instant::now());
            s.status.state = "connected".into();
            s.status.step = "Connected".into();
            s.status.message = "Chrome connection verified.".into();
            return Ok(json!({"ok":true}));
        }
        if kind == "group_revenue_claim"
            || kind == "group_revenue_batch"
            || kind == "group_revenue_complete"
            || kind == "group_revenue_error"
        {
            let workspace = s.workspace.clone();
            let transfer = s
                .group_revenue
                .as_mut()
                .ok_or_else(|| invalid("No Roblox Group Revenue refresh is active."))?;
            if Instant::now() > transfer.expires {
                transfer.state = "failed".into();
                transfer.message = "Roblox refresh expired. Start it again.".into();
                return Err(invalid("Roblox refresh expired."));
            }
            if kind == "group_revenue_claim" {
                if transfer.session.is_some()
                    || r["token"].as_str() != Some(transfer.token.as_str())
                {
                    return Err(invalid("Invalid Roblox refresh token."));
                }
                let session = secret();
                transfer.token.clear();
                transfer.session = Some(session.clone());
                return Ok(json!({"session":session,"groupId":transfer.group_id}));
            }
            if transfer.session.is_none() || r["session"].as_str() != transfer.session.as_deref() {
                return Err(invalid("Invalid Roblox refresh session."));
            }
            if kind == "group_revenue_error" {
                transfer.state = "failed".into();
                transfer.message = match r["reason"].as_str() { Some("unauthorized") => "Roblox login expired or this account cannot view the group sales.", Some("rate_limited") => "Roblox temporarily rate-limited the request. Try again later.", _ => "Roblox did not return a complete, supported response. Existing successful data was kept." }.into();
                return Ok(json!({"ok":true}));
            }
            if kind == "group_revenue_batch" {
                let passes: Vec<RobloxRevenuePass> = serde_json::from_value(r["passes"].clone())
                    .map_err(|_| invalid("Invalid Game Pass catalog batch."))?;
                let sales: Vec<RobloxGroupSale> = serde_json::from_value(r["sales"].clone())
                    .map_err(|_| invalid("Invalid transaction batch."))?;
                if passes.len() > 100
                    || sales.len() > 50
                    || transfer.passes.len() + passes.len() > 10_000
                    || transfer.sales.len() + sales.len() > 100_000
                {
                    return Err(invalid("Roblox refresh exceeded its safe data limit."));
                }
                if passes.iter().any(|pass| pass.group_id != transfer.group_id) {
                    return Err(invalid("Game Pass batch belongs to another group."));
                }
                transfer.passes.extend(passes);
                transfer.sales.extend(sales);
                return Ok(json!({"ok":true,"received":transfer.sales.len()}));
            }
            if kind == "group_revenue_complete" {
                let from = r["coverageFrom"].as_str().map(str::to_owned);
                let to = r["coverageTo"].as_str().map(str::to_owned);
                let summary = r.get("summary").filter(|v| v.is_object()).cloned();
                if let Some(summary) = &summary {
                    if summary.to_string().len() > 8_000 {
                        return Err(invalid("Roblox group summary was too large."));
                    }
                }
                transfer.coverage_from = from;
                transfer.coverage_to = to;
                transfer.summary = summary;
                let ws = workspace.ok_or_else(|| invalid("Workspace unavailable."))?;
                let result = ws.apply_roblox_group_api_refresh(
                    &transfer.group_id,
                    &transfer.passes,
                    &transfer.sales,
                    transfer.coverage_from.as_deref(),
                    transfer.coverage_to.as_deref(),
                    transfer.summary.as_ref(),
                )?;
                let unsupported = transfer
                    .summary
                    .as_ref()
                    .and_then(|value| value.get("unsupportedTransactions"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                transfer.state = "complete".into();
                transfer.message = if unsupported > 0 {
                    format!("Roblox catalog refreshed; {unsupported} transactions need CSV review, {} new sales, {} matched.", result.inserted, result.duplicates)
                } else {
                    format!(
                        "Roblox catalog refreshed; {} new sales, {} matched.",
                        result.inserted, result.duplicates
                    )
                };
                return Ok(
                    json!({"ok":true,"inserted":result.inserted,"duplicates":result.duplicates}),
                );
            }
        }
        if kind == "statistics_claim" || kind == "statistics_submit" || kind == "statistics_error" {
            let workspace = s.workspace.clone();
            let t = s
                .statistics
                .as_mut()
                .ok_or_else(|| invalid("No statistics request is active."))?;
            if Instant::now() > t.expires {
                return Err(invalid("Statistics request expired."));
            }
            if kind == "statistics_claim" {
                if t.session.is_some() || r["token"].as_str() != Some(t.token.as_str()) {
                    return Err(invalid("Invalid statistics token."));
                }
                let session = secret();
                t.token.clear();
                t.session = Some(session.clone());
                return Ok(json!({"session":session,"profileId":t.profile_id}));
            }
            if t.state != "running"
                || t.session.is_none()
                || r["session"].as_str() != t.session.as_deref()
            {
                return Err(invalid("Invalid statistics session."));
            }
            if kind == "statistics_error" {
                t.state = "failed".into();
                t.message = if r["reason"].as_str() == Some("account_mismatch") { "The signed-in ERLCX account does not match the selected user." } else { "Could not read the signed-in creator dashboard. Check your Chrome login and try again." }.into();
                return Ok(json!({"ok":true}));
            }
            if r["profileId"].as_str() != Some(t.profile_id.as_str()) {
                return Err(invalid(
                    "The signed-in ERLCX account does not match the selected user.",
                ));
            }
            let data: SalesData = serde_json::from_value(r["data"].clone())
                .map_err(|_| invalid("Invalid sales statistics."))?;
            workspace
                .ok_or_else(|| invalid("Workspace unavailable."))?
                .save_statistics_sales(&t.profile_id, &data)?;
            t.state = "complete".into();
            t.message = "Creator dashboard updated.".into();
            return Ok(json!({"ok":true}));
        }
        let Some(t) = s.transfer.as_mut() else {
            return Err(invalid("No approved handoff is active."));
        };
        if Instant::now() > t.expires {
            return Err(invalid(
                "Browser handoff expired. Start it again in Apex Studio.",
            ));
        }
        if kind == "claim" {
            if t.session.is_some() || r["token"].as_str() != Some(&t.token) {
                return Err(invalid("Handoff token is invalid or already used."));
            }
            let session = secret();
            t.session = Some(session.clone());
            t.expires = Instant::now() + Duration::from_secs(2 * 60 * 60);
            t.token.clear();
            return Ok(json!({"session":session,"size":t.size}));
        }
        if r["session"].as_str() != t.session.as_deref() || t.session.is_none() {
            return Err(invalid("Invalid handoff session."));
        }
        match kind {
            "chunk" => {
                use std::io::{Read, Seek, SeekFrom};
                let offset = r["offset"]
                    .as_u64()
                    .ok_or_else(|| invalid("Invalid chunk offset."))?
                    as usize;
                if offset != t.offset || offset >= t.size {
                    return Err(invalid("Unexpected chunk offset."));
                }
                let count = (t.size - offset).min(192 * 1024);
                let mut bytes = vec![0; count];
                let file = t.bundle.as_file_mut();
                file.seek(SeekFrom::Start(offset as u64))
                    .map_err(|e| Error::Storage(e.to_string()))?;
                file.read_exact(&mut bytes)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                t.offset += count;
                Ok(json!({"data":STANDARD.encode(bytes),"offset":t.offset,"done":t.offset==t.size}))
            }
            "status" => {
                let state = r["state"].as_str().unwrap_or("");
                let step = r["step"].as_str().unwrap_or("");
                if !["running", "paused", "review", "cancelled"].contains(&state)
                    || ![
                        "",
                        "Categories",
                        "Details",
                        "Media",
                        "Codes",
                        "Monetization",
                        "Submit",
                    ]
                    .contains(&step)
                {
                    return Err(invalid("Invalid browser status."));
                }
                let command = t.command.clone();
                if command == "resume" {
                    t.command = "run".into();
                }
                let post = t.post_id.clone();
                s.status = BrowserStatus {
                    connected: true,
                    state: state.into(),
                    step: step.into(),
                    message: r["message"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(500)
                        .collect(),
                    post_id: Some(post),
                };
                if state == "review" || state == "cancelled" {
                    s.transfer = None;
                }
                Ok(json!({"command":command}))
            }
            _ => Err(invalid("Unsupported bridge request.")),
        }
    }
}
#[cfg(windows)]
fn initial_pipe(name: &str) -> Result<tokio::net::windows::named_pipe::NamedPipeServer, Error> {
    tauri::async_runtime::block_on(async {
        tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(name)
            .map_err(|e| Error::Storage(e.to_string()))
    })
}
#[cfg(windows)]
pub fn setup(app: &tauri::AppHandle, workspace: Workspace) -> Result<(), Error> {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::windows::named_pipe::ServerOptions,
    };
    let bridge = BrowserBridge::default();
    let mut config = protocol::config().unwrap_or_default();
    config.pipe = format!(r"\\.\pipe\ApexStudio-{}", secret());
    let dir = protocol::directory().map_err(|e| Error::Storage(e.to_string()))?;
    std::fs::create_dir_all(&dir).map_err(|e| Error::Storage(e.to_string()))?;
    std::fs::write(
        dir.join("settings.json"),
        serde_json::to_vec(&config).map_err(|e| Error::Storage(e.to_string()))?,
    )
    .map_err(|e| Error::Storage(e.to_string()))?;
    bridge.lock()?.config = config.clone();
    bridge.lock()?.workspace = Some(workspace);
    app.manage(bridge.clone());
    // Acquire the endpoint before exposing it. first_pipe_instance prevents accidental sharing.
    let initial = initial_pipe(&config.pipe)?;
    tauri::async_runtime::spawn(async move {
        let mut next = Some(initial);
        loop {
            let Some(mut pipe) = next.take() else {
                break;
            };
            if pipe.connect().await.is_err() {
                break;
            }
            next = ServerOptions::new()
                .reject_remote_clients(true)
                .create(&config.pipe)
                .ok();
            let bridge = bridge.clone();
            tauri::async_runtime::spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(10), async move {
                    let length = pipe.read_u32_le().await? as usize;
                    if length == 0 || length > 16 * 1024 {
                        return Err(std::io::Error::other("Invalid request size."));
                    }
                    let mut data = vec![0; length];
                    pipe.read_exact(&mut data).await?;
                    let response = match serde_json::from_slice(&data) {
                        Ok(r) => bridge.respond(r),
                        Err(_) => json!({"error":"Invalid request."}),
                    };
                    let bytes = serde_json::to_vec(&response)?;
                    pipe.write_u32_le(bytes.len() as u32).await?;
                    pipe.write_all(&bytes).await?;
                    pipe.flush().await?;
                    Ok::<_, std::io::Error>(())
                })
                .await;
            });
        }
    });
    Ok(())
}
#[cfg(not(windows))]
pub fn setup(app: &tauri::AppHandle, workspace: Workspace) -> Result<(), Error> {
    let bridge = BrowserBridge::default();
    bridge.lock()?.workspace = Some(workspace);
    app.manage(bridge);
    Ok(())
}
#[tauri::command]
pub fn chrome_settings(
    bridge: tauri::State<'_, BrowserBridge>,
) -> Result<protocol::ChromeConfig, CommandError> {
    let mut c = bridge.lock()?.config.clone();
    c.pipe.clear();
    Ok(c)
}
#[tauri::command]
pub fn browser_status(
    bridge: tauri::State<'_, BrowserBridge>,
) -> Result<BrowserStatus, CommandError> {
    Ok(bridge.snapshot()?)
}
fn launch(config: &protocol::ChromeConfig, fragment: &str) -> Result<(), Error> {
    let mut cmd = std::process::Command::new(&config.chrome_path);
    if !config.user_data_dir.trim().is_empty() {
        cmd.arg(format!("--user-data-dir={}", config.user_data_dir));
    }
    cmd.arg(format!("--profile-directory={}", config.profile))
        .arg(format!(
            "chrome-extension://{}/handoff.html#{fragment}",
            config.extension_id
        ));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.spawn().map_err(|e| Error::Storage(e.to_string()))?;
    Ok(())
}
#[cfg(windows)]
fn register_opera_host(manifest: &Path) -> Result<(), Error> {
    use std::os::windows::process::CommandExt;

    // Opera's Windows documentation requires the Chrome-compatible machine registry key.
    // Ask for elevation only when Opera is selected; the host remains scoped to this extension ID.
    let key = r"HKLM\SOFTWARE\Google\Chrome\NativeMessagingHosts\com.apexstudio.erlcx";
    let args = format!(
        "ADD \"{key}\" /ve /t REG_SZ /d \"{}\" /f",
        manifest.display()
    );
    let escaped_args = args.replace('\'', "''");
    let script = format!(
        "$p = Start-Process -FilePath \"$env:SystemRoot\\System32\\reg.exe\" -ArgumentList '{escaped_args}' -Verb RunAs -Wait -PassThru; exit $p.ExitCode"
    );
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000)
        .status()
        .map_err(|e| Error::Storage(e.to_string()))?;
    if !status.success() {
        return Err(invalid("Opera GX native messaging setup was cancelled or failed. Approve the Windows permission prompt and save the browser settings again.").into());
    }
    Ok(())
}
#[tauri::command]
pub fn save_chrome_settings(
    app: tauri::AppHandle,
    mut config: protocol::ChromeConfig,
    bridge: tauri::State<'_, BrowserBridge>,
) -> Result<(), CommandError> {
    if !protocol::valid_extension(&config.extension_id)
        || !Path::new(&config.chrome_path).is_file()
        || Path::new(&config.chrome_path)
            .file_name()
            .is_none_or(|n| !n.to_string_lossy().to_ascii_lowercase().ends_with(".exe"))
        || (!config.user_data_dir.trim().is_empty() && !Path::new(&config.user_data_dir).is_dir())
        || config.profile.is_empty()
        || config.profile.len() > 80
        || config
            .profile
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && c != ' ' && c != '-' && c != '_')
    {
        return Err(invalid(
            "Choose a Chromium browser executable, an existing user data folder, a profile name and the installed extension ID.",
        )
        .into());
    }
    let mut state = bridge.lock()?;
    if state.transfer.is_some()
        || state.pending.is_some()
        || state
            .group_revenue
            .as_ref()
            .is_some_and(|t| t.state == "running" && Instant::now() < t.expires)
        || state
            .statistics
            .as_ref()
            .is_some_and(|t| t.state == "running" && Instant::now() < t.expires)
    {
        return Err(invalid("Finish or cancel the active browser handoff first.").into());
    }
    config.pipe = state.config.pipe.clone();
    let dir = protocol::directory().map_err(|e| Error::Storage(e.to_string()))?;
    let adjacent_helper = std::env::current_exe()
        .map_err(|e| Error::Storage(e.to_string()))?
        .with_file_name("apex-chrome-bridge.exe");
    let helper = if adjacent_helper.is_file() {
        adjacent_helper
    } else {
        app.path()
            .resolve(
                "apex-chrome-bridge.exe",
                tauri::path::BaseDirectory::Resource,
            )
            .map_err(|e| Error::Storage(e.to_string()))?
    };
    if !helper.exists() {
        return Err(invalid("The Chrome bridge executable is missing beside Apex Studio.").into());
    }
    let manifest = dir.join("com.apexstudio.erlcx.json");
    let bytes=serde_json::to_vec_pretty(&json!({"name":protocol::HOST,"description":"Apex Studio ERLCX handoff","path":helper,"type":"stdio","allowed_origins":[format!("chrome-extension://{}/",config.extension_id)]})).map_err(|e|Error::Storage(e.to_string()))?;
    std::fs::write(&manifest, bytes).map_err(|e| Error::Storage(e.to_string()))?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Chromium browsers use the Chrome host location as a compatibility fallback.
        // Edge also checks its product-specific key. Opera GX documents the Chrome key.
        for key in [
            r"HKCU\Software\Google\Chrome\NativeMessagingHosts\com.apexstudio.erlcx",
            r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.apexstudio.erlcx",
        ] {
            let status = std::process::Command::new("reg.exe")
                .args(["ADD", key, "/ve", "/t", "REG_SZ", "/d"])
                .arg(&manifest)
                .arg("/f")
                .creation_flags(0x08000000)
                .output()
                .map_err(|e| Error::Storage(e.to_string()))?;
            if !status.status.success() {
                return Err(
                    invalid("Cannot register the browser bridge for this Windows user.").into(),
                );
            }
        }
        if Path::new(&config.chrome_path)
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("opera.exe"))
        {
            register_opera_host(&manifest)?;
        }
    }
    std::fs::write(
        dir.join("settings.json"),
        serde_json::to_vec(&config).map_err(|e| Error::Storage(e.to_string()))?,
    )
    .map_err(|e| Error::Storage(e.to_string()))?;
    state.config = config;
    state.checked = None;
    Ok(())
}
#[tauri::command]
pub fn check_browser(bridge: tauri::State<'_, BrowserBridge>) -> Result<(), CommandError> {
    let mut state = bridge.lock()?;
    if !protocol::valid_extension(&state.config.extension_id) {
        return Err(invalid("Set up the Chrome extension first.").into());
    }
    let token = secret();
    state.check = Some((token.clone(), Instant::now() + Duration::from_secs(60)));
    state.checked = None;
    state.status = BrowserStatus {
        connected: false,
        state: "checking".into(),
        step: "Connection check".into(),
        message: "Waiting for the Chrome extension to confirm the connection…".into(),
        post_id: None,
    };
    if let Err(error) = launch(&state.config, &format!("check={token}")) {
        state.check = None;
        state.status.state = "failed".into();
        state.status.message = format!("Could not open the configured browser: {error}");
        return Err(error.into());
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsBrowserStatus {
    state: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRevenueBrowserStatus {
    state: String,
    message: String,
}

#[tauri::command]
pub fn group_revenue_browser_status(
    bridge: tauri::State<'_, BrowserBridge>,
) -> Result<GroupRevenueBrowserStatus, CommandError> {
    let mut state = bridge.lock()?;
    let transfer = state.group_revenue.as_mut();
    if let Some(t) = transfer {
        if t.state == "running" && Instant::now() > t.expires {
            t.state = "failed".into();
            t.message = "Roblox refresh expired. Start it again.".into();
        }
        return Ok(GroupRevenueBrowserStatus {
            state: t.state.clone(),
            message: t.message.clone(),
        });
    }
    Ok(GroupRevenueBrowserStatus {
        state: "idle".into(),
        message: String::new(),
    })
}

#[tauri::command]
pub fn start_group_revenue_browser(
    group_id: String,
    bridge: tauri::State<'_, BrowserBridge>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    if !workspace
        .roblox_group_revenue()?
        .groups
        .iter()
        .any(|group| group.group_id == group_id)
    {
        return Err(
            invalid("Save this group and its experiences before refreshing Roblox data.").into(),
        );
    }
    let mut state = bridge.lock()?;
    if !state
        .checked
        .is_some_and(|t| t.elapsed() < Duration::from_secs(300))
    {
        return Err(invalid("Verify the Chrome connection in Settings first.").into());
    }
    if state.transfer.is_some()
        || state.pending.is_some()
        || state
            .statistics
            .as_ref()
            .is_some_and(|t| t.state == "running" && Instant::now() < t.expires)
        || state
            .group_revenue
            .as_ref()
            .is_some_and(|t| t.state == "running" && Instant::now() < t.expires)
    {
        return Err(invalid("Another Chrome operation is already active.").into());
    }
    let token = secret();
    let config = state.config.clone();
    state.group_revenue = Some(GroupRevenueTransfer {
        token: token.clone(),
        session: None,
        group_id,
        expires: Instant::now() + Duration::from_secs(20 * 60),
        state: "running".into(),
        message: "Reading Roblox group data…".into(),
        passes: Vec::new(),
        sales: Vec::new(),
        coverage_from: None,
        coverage_to: None,
        summary: None,
    });
    drop(state);
    if let Err(error) = launch(&config, &format!("group-revenue={token}")) {
        bridge.lock()?.group_revenue = None;
        return Err(error.into());
    }
    Ok(())
}

#[tauri::command]
pub fn statistics_browser_status(
    bridge: tauri::State<'_, BrowserBridge>,
) -> Result<StatisticsBrowserStatus, CommandError> {
    let mut state = bridge.lock()?;
    if let Some(t) = state.statistics.as_mut() {
        if t.state == "running" && Instant::now() > t.expires {
            t.state = "failed".into();
            t.message = "Chrome connection timed out. Try again.".into();
        }
        return Ok(StatisticsBrowserStatus {
            state: t.state.clone(),
            message: t.message.clone(),
        });
    }
    Ok(StatisticsBrowserStatus {
        state: "idle".into(),
        message: String::new(),
    })
}

#[tauri::command]
pub fn start_statistics_browser(
    bridge: tauri::State<'_, BrowserBridge>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let id = workspace
        .statistics_profile()?
        .ok_or_else(|| invalid("Set a Roblox user ID in Settings first."))?;
    let mut state = bridge.lock()?;
    if !state
        .checked
        .is_some_and(|t| t.elapsed() < Duration::from_secs(300))
    {
        return Err(invalid("Verify the Chrome connection in Settings first.").into());
    }
    if state.pending.is_some()
        || state
            .transfer
            .as_ref()
            .is_some_and(|t| Instant::now() < t.expires)
        || state
            .statistics
            .as_ref()
            .is_some_and(|t| t.state == "running" && Instant::now() < t.expires)
        || state
            .group_revenue
            .as_ref()
            .is_some_and(|t| t.state == "running" && Instant::now() < t.expires)
    {
        return Err(invalid("A Chrome operation is already active.").into());
    }
    let token = secret();
    let config = state.config.clone();
    state.statistics = Some(StatisticsTransfer {
        token: token.clone(),
        session: None,
        profile_id: id,
        expires: Instant::now() + Duration::from_secs(120),
        state: "running".into(),
        message: "Opening creator dashboard…".into(),
    });
    drop(state);
    if let Err(e) = launch(&config, &format!("statistics={token}")) {
        bridge.lock()?.statistics = None;
        return Err(e.into());
    }
    Ok(())
}
#[tauri::command]
pub async fn prepare_browser(
    id: String,
    revision: u64,
    reviewed: bool,
    workspace: tauri::State<'_, Workspace>,
    bridge: tauri::State<'_, BrowserBridge>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    let bridge = bridge.inner().clone();
    work(move || {
        let token = secret();
        let config = {
            let mut state = bridge.lock()?;
            if !state
                .checked
                .is_some_and(|t| t.elapsed() < Duration::from_secs(300))
            {
                return Err(invalid("Verify the Chrome connection in Settings first."));
            }
            if state
                .statistics
                .as_ref()
                .is_some_and(|t| t.state == "running" && Instant::now() < t.expires)
                || state.pending.is_some()
                || state
                    .group_revenue
                    .as_ref()
                    .is_some_and(|t| t.state == "running" && Instant::now() < t.expires)
                || state
                    .transfer
                    .as_ref()
                    .is_some_and(|t| Instant::now() < t.expires)
            {
                return Err(invalid("A browser handoff is already active."));
            }
            state.pending = Some(token.clone());
            state.config.clone()
        };
        let prepared = (|| {
            let temp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
            let path = temp.path().join("post.apexpublish");
            ws.export_post(&id, revision, reviewed, &path)?;
            let bundle =
                tempfile::NamedTempFile::new().map_err(|e| Error::Storage(e.to_string()))?;
            std::fs::copy(&path, bundle.path()).map_err(|e| Error::Storage(e.to_string()))?;
            let size = bundle
                .as_file()
                .metadata()
                .map_err(|e| Error::Storage(e.to_string()))?
                .len() as usize;
            Ok::<_, Error>((bundle, size))
        })();
        let mut state = bridge.lock()?;
        if state.pending.as_deref() != Some(&token) {
            return Err(invalid("Workspace changed. Start a new browser handoff."));
        }
        state.pending = None;
        let (bundle, size) = prepared?;
        state.transfer = Some(Transfer {
            token: token.clone(),
            session: None,
            expires: Instant::now() + Duration::from_secs(120),
            bundle,
            offset: 0,
            size,
            post_id: id.clone(),
            command: "run".into(),
        });
        state.status = BrowserStatus {
            connected: true,
            state: "running".into(),
            step: "".into(),
            message: "Opening Chrome…".into(),
            post_id: Some(id),
        };
        drop(state);
        if let Err(error) = launch(&config, &format!("token={token}")) {
            let mut state = bridge.lock()?;
            state.transfer = None;
            state.status.state = "failed".into();
            return Err(error);
        }
        Ok(())
    })
    .await
}
#[tauri::command]
pub fn control_browser(
    action: String,
    bridge: tauri::State<'_, BrowserBridge>,
) -> Result<(), CommandError> {
    let mut s = bridge.lock()?;
    if !["resume", "cancel"].contains(&action.as_str()) {
        return Err(invalid("Invalid browser action.").into());
    }
    let t = s
        .transfer
        .as_mut()
        .ok_or_else(|| invalid("No browser handoff is active."))?;
    if action == "resume" {
        t.expires = Instant::now() + Duration::from_secs(2 * 60 * 60);
    }
    t.command = action.clone();
    if action == "cancel" {
        s.status.state = "cancelled".into();
        s.status.message = "Browser handoff cancelled.".into();
        s.transfer = None;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn synchronous_startup_creates_pipe_inside_the_io_runtime() {
        let name = format!(r"\\.\pipe\ApexStudio-startup-{}", secret());
        std::thread::spawn(move || initial_pipe(&name).unwrap())
            .join()
            .unwrap();
    }
    fn fixture() -> BrowserBridge {
        let bridge = BrowserBridge::default();
        let mut s = bridge.lock().unwrap();
        s.config.extension_id = "a".repeat(32);
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), vec![42; 400_000]).unwrap();
        s.transfer = Some(Transfer {
            token: "approved".into(),
            session: None,
            expires: Instant::now() + Duration::from_secs(120),
            bundle: file,
            offset: 0,
            size: 400_000,
            post_id: "post".into(),
            command: "run".into(),
        });
        drop(s);
        bridge
    }
    fn request(kind: &str) -> Value {
        json!({"kind":kind,"origin":format!("chrome-extension://{}/","a".repeat(32))})
    }
    #[test]
    fn tokens_are_one_use_and_chunks_are_bounded_and_ordered() {
        let b = fixture();
        let mut r = request("claim");
        r["token"] = "approved".into();
        let session = b.process(r.clone()).unwrap()["session"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(b.process(r).is_err());
        let mut chunk = request("chunk");
        chunk["session"] = session.into();
        chunk["offset"] = 0.into();
        let part = b.process(chunk.clone()).unwrap();
        assert!(part.to_string().len() < protocol::MAX_FRAME);
        assert_eq!(part["offset"], 192 * 1024);
        assert!(b.process(chunk.clone()).is_err());
        chunk["offset"] = part["offset"].clone();
        assert!(b.process(chunk).is_ok());
    }
    #[test]
    fn origin_expiry_and_unknown_requests_never_expose_a_package() {
        let b = fixture();
        let mut r = request("claim");
        r["token"] = "approved".into();
        r["origin"] = "chrome-extension://wrong/".into();
        assert!(b.process(r).is_err());
        assert!(b.process(request("read_file")).is_err());
        b.lock().unwrap().transfer.as_mut().unwrap().expires =
            Instant::now() - Duration::from_secs(1);
        let mut r = request("claim");
        r["token"] = "approved".into();
        assert!(b.process(r).is_err());
        assert_eq!(b.snapshot().unwrap().state, "expired");
    }
    #[test]
    fn check_is_bound_to_an_expiring_challenge() {
        let b = fixture();
        b.lock().unwrap().check = Some(("nonce".into(), Instant::now() + Duration::from_secs(60)));
        let mut r = request("check");
        r["token"] = "nonce".into();
        assert!(b.process(r.clone()).is_ok());
        assert!(b.snapshot().unwrap().connected);
        assert!(b.process(r).is_err());
    }
    #[test]
    fn statistics_are_bound_to_one_use_token_session_and_profile() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path()).unwrap();
        ws.set_statistics_profile("123").unwrap();
        let b = fixture();
        {
            let mut state = b.lock().unwrap();
            state.workspace = Some(ws.clone());
            state.statistics = Some(StatisticsTransfer {
                token: "once".into(),
                session: None,
                profile_id: "123".into(),
                expires: Instant::now() + Duration::from_secs(60),
                state: "running".into(),
                message: String::new(),
            });
        }
        let mut claim = request("statistics_claim");
        claim["token"] = "once".into();
        let session = b.process(claim.clone()).unwrap()["session"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(b.process(claim).is_err());
        let mut submit = request("statistics_submit");
        submit["session"] = session.into();
        submit["profileId"] = "wrong".into();
        submit["data"] =
            json!({"sales":1,"allTimeSales":2,"netRobux":70,"grossRobux":100,"series":[]});
        assert!(b.process(submit.clone()).is_err());
        submit["profileId"] = "123".into();
        submit["data"]["buyer"] = "private".into();
        assert!(b.process(submit.clone()).is_err());
        submit["data"].as_object_mut().unwrap().remove("buyer");
        assert!(b.process(submit.clone()).is_ok());
        assert!(b.process(submit).is_err());
        assert_eq!(
            ws.statistics_sales("123").unwrap().unwrap().data.net_robux,
            70
        );
    }
}
