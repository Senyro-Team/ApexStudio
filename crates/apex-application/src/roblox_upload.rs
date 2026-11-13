//! Roblox decal upload transport shared by the desktop adapter and CLI.
//!
//! This module owns the HTTP, rate-budget, queue-execution and response rules
//! shared by the desktop tray worker and standalone CLI worker.

use crate::RobloxCredentials;
use apex_core::Error;
use apex_storage::{queue_now, DecalUpload, UploadJob, Workspace};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Response,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

const ASSET_API_BASE: &str = "https://apis.roblox.com/assets/v1";

fn bad(message: &str) -> Error {
    Error::InvalidContent(message.into())
}

fn keep_first_error(slot: &mut Option<Error>, error: Error) {
    if slot.is_none() {
        *slot = Some(error);
    }
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

#[derive(Default)]
struct RateBudget {
    until: i64,
    remaining: Option<i64>,
    failures: u32,
    reset_at: i64,
}

/// Shared Roblox request, rate-limit, and durable pass-processing service.
///
/// Adapters choose when to start or stop the worker and how to present progress;
/// this service owns leased task execution and persisted queue transitions.
pub struct RobloxUploadService {
    http: Client,
    base_url: String,
    credential_loader: fn() -> Result<Option<RobloxCredentials>, Error>,
    budgets: Mutex<HashMap<String, RateBudget>>,
    concurrency: AtomicUsize,
    clean_since: Mutex<i64>,
    last_transfer: AtomicI64,
}

impl RobloxUploadService {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            http: client()?,
            base_url: ASSET_API_BASE.into(),
            credential_loader: crate::load_roblox_credentials,
            budgets: Mutex::new(HashMap::new()),
            concurrency: AtomicUsize::new(5),
            clean_since: Mutex::new(queue_now()),
            last_transfer: AtomicI64::new(0),
        })
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency.load(Ordering::Relaxed)
    }

    pub fn next_allowed_at(&self) -> i64 {
        self.budgets
            .lock()
            .map(|budgets| {
                budgets
                    .values()
                    .map(|budget| budget.until)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or_else(|_| queue_now() + 1000)
    }

    pub fn reset_concurrency(&self) {
        self.concurrency.store(5, Ordering::Relaxed);
        if let Ok(mut since) = self.clean_since.lock() {
            *since = queue_now();
        }
    }

    #[cfg(test)]
    fn blocked(&self, key: &str) -> i64 {
        self.budgets
            .lock()
            .ok()
            .and_then(|budgets| budgets.get(key).map(|budget| budget.until))
            .unwrap_or(0)
    }

    pub fn reserve(&self, key: &str) -> i64 {
        let now = queue_now();
        let Ok(mut budgets) = self.budgets.lock() else {
            return now + 1000;
        };
        let budget = budgets.entry(key.into()).or_default();
        if budget.until > now {
            return budget.until;
        }
        if budget.remaining == Some(0) {
            if budget.reset_at > now {
                budget.until = budget.reset_at;
                return budget.reset_at;
            }
            budget.remaining = None;
        }
        if let Some(remaining) = budget.remaining.as_mut() {
            *remaining = (*remaining - 1).max(0);
        }
        0
    }

    pub fn observe(&self, key: &str, response: &Response) -> i64 {
        self.observe_headers(key, response.status().as_u16(), response.headers())
    }

    pub fn observe_headers(&self, key: &str, status: u16, headers: &HeaderMap) -> i64 {
        let now = queue_now();
        if (200..300).contains(&status) {
            self.last_transfer.store(now, Ordering::Relaxed);
        }
        let mut until = 0;
        if let Ok(mut budgets) = self.budgets.lock() {
            let budget = budgets.entry(key.into()).or_default();
            let seconds = |name: &str| {
                headers
                    .get(name)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.split(',').next())
                    .and_then(|value| value.trim().parse::<i64>().ok())
            };
            let remaining = seconds("x-ratelimit-remaining").map(|number| number.max(0));
            budget.remaining = if budget.reset_at > now {
                match (budget.remaining, remaining) {
                    (Some(previous), Some(current)) => Some(previous.min(current)),
                    (previous, None) => previous,
                    (_, current) => current,
                }
            } else {
                remaining
            };
            if remaining.is_some() {
                budget.reset_at =
                    now + seconds("x-ratelimit-reset").unwrap_or(30).clamp(0, 86400) * 1000;
            }
            if status == 429 {
                budget.failures = (budget.failures + 1).min(6);
                let retry = headers
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| {
                        value
                            .parse::<i64>()
                            .ok()
                            .map(|number| number.clamp(0, 86400) * 1000)
                            .or_else(|| {
                                httpdate::parse_http_date(value)
                                    .ok()
                                    .and_then(|date| {
                                        date.duration_since(std::time::SystemTime::now()).ok()
                                    })
                                    .map(|duration| duration.as_millis() as i64)
                            })
                    })
                    .unwrap_or((1i64 << budget.failures) * 1000);
                let reset = seconds("x-ratelimit-reset").unwrap_or(0).clamp(0, 86400) * 1000;
                budget.until = now + retry.max(reset) + now.rem_euclid(401) + 100;
                self.concurrency.store(
                    (self.concurrency.load(Ordering::Relaxed) / 2).max(1),
                    Ordering::Relaxed,
                );
            } else if budget.remaining == Some(0) {
                budget.until =
                    now + seconds("x-ratelimit-reset").unwrap_or(30).clamp(0, 86400) * 1000;
            } else if (200..300).contains(&status) {
                budget.failures = 0;
            }
            until = budget.until;
        }
        if status == 429 {
            if let Ok(mut clean_since) = self.clean_since.lock() {
                *clean_since = now;
            }
        }
        until
    }

    pub fn ramp(&self, now: i64) {
        if let Ok(mut clean_since) = self.clean_since.lock() {
            if now - self.last_transfer.load(Ordering::Relaxed) < 2000
                && now - *clean_since >= 30_000
                && self
                    .budgets
                    .lock()
                    .map(|budgets| {
                        budgets
                            .values()
                            .any(|budget| budget.remaining.unwrap_or(0) > 10)
                            && budgets.values().all(|budget| budget.until <= now)
                    })
                    .unwrap_or(false)
            {
                self.concurrency
                    .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                        Some((current + 1).min(10))
                    })
                    .ok();
                *clean_since = now;
            }
        }
    }

    /// Execute one leased queue task and persist the resulting pass state.
    pub async fn execute(
        &self,
        workspace: &Workspace,
        job: &UploadJob,
        poll: bool,
        key: &str,
        credentials: &RobloxCredentials,
        stopping: bool,
    ) {
        self.execute_at(workspace, job, poll, key, credentials, stopping)
            .await;
    }

    async fn execute_at(
        &self,
        workspace: &Workspace,
        job: &UploadJob,
        poll: bool,
        key: &str,
        credentials: &RobloxCredentials,
        stopping: bool,
    ) {
        if stopping {
            let _ = workspace.upload_defer(
                job,
                if poll { "processing" } else { "queued" },
                0,
                "Paused while Apex Studio exits.",
            );
            return;
        }
        let request: Result<Response, Error> = if poll {
            let Some(path) = &job.operation else {
                let _ = workspace.upload_defer(
                    job,
                    "needs_review",
                    0,
                    "Missing operation reference. Check Creator Dashboard.",
                );
                return;
            };
            if operation_path(&json!({"path":path})).is_err() {
                let _ =
                    workspace.upload_defer(job, "needs_review", 0, "Invalid operation reference.");
                return;
            }
            self.http
                .get(format!("{}/{path}", self.base_url))
                .headers(match headers(credentials) {
                    Ok(headers) => headers,
                    Err(_) => return,
                })
                .send()
                .await
                .map_err(|_| bad("Roblox could not check this upload operation."))
        } else {
            let db = workspace.clone();
            let hash = job.hash.clone();
            let bytes = match tokio::task::spawn_blocking(move || db.decal_bytes(&hash)).await {
                Ok(Ok(bytes)) => bytes,
                Ok(Err(error)) => {
                    let _ = workspace.upload_defer(job, "failed", 0, &error.to_string());
                    return;
                }
                Err(_) => {
                    let _ = workspace.upload_defer(
                        job,
                        "failed",
                        0,
                        "Could not read the queued texture from workspace storage.",
                    );
                    return;
                }
            };
            create_decal(
                &self.http,
                &format!("{}/assets", self.base_url),
                credentials,
                &job.name,
                bytes,
            )
            .await
        };
        let response = match request {
            Ok(response) => response,
            Err(_) => {
                let _ = workspace.upload_defer(
                    job,
                    if poll { "processing" } else { "needs_review" },
                    if poll { 8000 } else { 0 },
                    if poll {
                        "Connection interrupted; status will be checked again."
                    } else {
                        "Upload result unknown. Check Creator Dashboard; this pass will not be repeated automatically."
                    },
                );
                return;
            }
        };
        let status = response.status().as_u16();
        let until = self.observe(key, &response);
        if status == 429 {
            let _ = workspace.upload_defer(
                job,
                if poll { "processing" } else { "queued" },
                (until - queue_now()).max(2000),
                "Waiting for Roblox rate limit.",
            );
            return;
        }
        if status == 401 || status == 403 {
            let _ = workspace.upload_defer(
                job,
                if poll { "processing" } else { "queued" },
                0,
                "Roblox rejected the key or its permissions. Verify Settings and Resume.",
            );
            let _ = workspace.pause_upload_account(&job.account_id);
            return;
        }
        if !(200..300).contains(&status) {
            let uncertain = !poll && (status >= 500 || status == 408);
            let _ = workspace.upload_defer(
                job,
                if uncertain {
                    "needs_review"
                } else if poll && status >= 500 {
                    "processing"
                } else {
                    "failed"
                },
                8000,
                "Roblox rejected or could not complete this request. Review the texture and retry if appropriate.",
            );
            return;
        }
        let value = match body(response).await {
            Ok(value) => value,
            Err(_) => {
                let _ = workspace.upload_defer(
                    job,
                    if poll { "processing" } else { "needs_review" },
                    8000,
                    "Roblox response was interrupted. Check the saved operation before retrying.",
                );
                return;
            }
        };
        if !poll {
            match operation_path(&value) {
                Ok(path) => {
                    let _ = workspace.upload_operation(job, &path);
                }
                Err(_) => {
                    let _ = workspace.upload_defer(
                        job,
                        "needs_review",
                        0,
                        "Roblox did not return an operation. Check Creator Dashboard.",
                    );
                }
            }
            return;
        }
        if value.get("error").is_some_and(|error| !error.is_null()) {
            let _ = workspace.fail_upload_operation(job);
            return;
        }
        if value["done"] != true {
            let _ = workspace.upload_defer(
                job,
                "processing",
                [2000, 4000, 8000, 15000][(job.poll_attempts as usize).min(3)],
                "Roblox is processing this upload.",
            );
            return;
        }
        let mut upload = DecalUpload {
            account_id: job.account_id.clone(),
            hash: job.hash.clone(),
            state: String::new(),
            operation: job.operation.clone(),
            asset_id: None,
            moderation: "unknown".into(),
            message: String::new(),
        };
        match apply_operation(&mut upload, &value) {
            Ok(()) => {
                if let Some(asset_id) = upload.asset_id {
                    let _ = workspace.upload_pass_complete(job, &asset_id, &upload.moderation);
                } else if upload.state == "failed" {
                    let _ = workspace.fail_upload_operation(job);
                }
            }
            Err(error) => {
                let _ = workspace.upload_defer(job, "needs_review", 0, &error.to_string());
            }
        }
    }

    /// Run active queue batches until they complete, pause, or the caller
    /// requests a graceful stop. The caller owns terminal/event presentation.
    pub async fn run_until_idle<F>(
        self: Arc<Self>,
        workspace: Workspace,
        stopping: Arc<AtomicBool>,
        on_update: F,
    ) -> Result<apex_storage::QueueSnapshot, Error>
    where
        F: Fn(&apex_storage::QueueSnapshot, usize) + Send + Sync + 'static,
    {
        let mut tasks = tokio::task::JoinSet::new();
        let mut create_count = 0usize;
        let mut poll_count = 0usize;
        let mut stop_applied = false;
        let mut worker_error = None;

        loop {
            if stopping.load(Ordering::Relaxed) && !stop_applied {
                let db = workspace.clone();
                match tokio::task::spawn_blocking(move || db.control_upload_batch(None, "pause"))
                    .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => keep_first_error(&mut worker_error, error),
                    Err(error) => {
                        keep_first_error(&mut worker_error, Error::Storage(error.to_string()))
                    }
                }
                stop_applied = true;
            }
            if !stopping.load(Ordering::Relaxed) {
                self.ramp(queue_now());
                for poll in [true, false] {
                    let in_flight = if poll { poll_count } else { create_count };
                    let cap = if poll { 2 } else { self.concurrency() };
                    let mut available = cap.saturating_sub(in_flight);
                    while available > 0 && !stopping.load(Ordering::Relaxed) {
                        // Lease one job at a time. If a storage operation fails,
                        // no other jobs are left stranded in submitting/polling.
                        let db = workspace.clone();
                        let mut jobs = match tokio::task::spawn_blocking(move || {
                            db.lease_upload_tasks(poll, 1)
                        })
                        .await
                        {
                            Ok(Ok(jobs)) => jobs,
                            Ok(Err(error)) => {
                                keep_first_error(&mut worker_error, error);
                                stopping.store(true, Ordering::Relaxed);
                                break;
                            }
                            Err(error) => {
                                keep_first_error(
                                    &mut worker_error,
                                    Error::Storage(error.to_string()),
                                );
                                stopping.store(true, Ordering::Relaxed);
                                break;
                            }
                        };
                        let Some(job) = jobs.pop() else {
                            break;
                        };
                        if stopping.load(Ordering::Relaxed) {
                            let db = workspace.clone();
                            let deferred = tokio::task::spawn_blocking(move || {
                                db.upload_defer(
                                    &job,
                                    if poll { "processing" } else { "queued" },
                                    0,
                                    "Queue paused before this task started.",
                                )
                            })
                            .await;
                            match deferred {
                                Ok(Ok(())) => {}
                                Ok(Err(error)) => keep_first_error(&mut worker_error, error),
                                Err(error) => keep_first_error(
                                    &mut worker_error,
                                    Error::Storage(error.to_string()),
                                ),
                            }
                            break;
                        }
                        let key = format!(
                            "{}:{}",
                            job.account_id,
                            if poll { "poll" } else { "create" }
                        );
                        let until = self.reserve(&key);
                        if until > queue_now() {
                            let db = workspace.clone();
                            let wait = (until - queue_now()).max(0);
                            let deferred = tokio::task::spawn_blocking(move || {
                                db.upload_defer(
                                    &job,
                                    if poll { "processing" } else { "queued" },
                                    wait,
                                    "Waiting for Roblox rate limit.",
                                )
                            })
                            .await;
                            match deferred {
                                Ok(Ok(())) => {}
                                Ok(Err(error)) => {
                                    keep_first_error(&mut worker_error, error);
                                    stopping.store(true, Ordering::Relaxed);
                                    break;
                                }
                                Err(error) => {
                                    keep_first_error(
                                        &mut worker_error,
                                        Error::Storage(error.to_string()),
                                    );
                                    stopping.store(true, Ordering::Relaxed);
                                    break;
                                }
                            }
                            available -= 1;
                            continue;
                        }

                        if poll {
                            poll_count += 1;
                        } else {
                            create_count += 1;
                        }
                        available -= 1;
                        let service = self.clone();
                        let db = workspace.clone();
                        tasks.spawn(async move {
                            let result = service.run_task(&db, &job, poll, &key).await;
                            (poll, result)
                        });
                    }
                    if stopping.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }

            let db = workspace.clone();
            let snapshot = match tokio::task::spawn_blocking(move || db.upload_snapshot()).await {
                Ok(Ok(snapshot)) => snapshot,
                Ok(Err(error)) => {
                    keep_first_error(&mut worker_error, error);
                    stopping.store(true, Ordering::Relaxed);
                    apex_storage::QueueSnapshot::default()
                }
                Err(error) => {
                    keep_first_error(&mut worker_error, Error::Storage(error.to_string()));
                    stopping.store(true, Ordering::Relaxed);
                    apex_storage::QueueSnapshot::default()
                }
            };
            on_update(&snapshot, tasks.len());

            if tasks.is_empty()
                && create_count + poll_count == 0
                && !snapshot
                    .batches
                    .iter()
                    .any(|batch| batch.state == "running")
            {
                return match worker_error {
                    Some(error) => Err(error),
                    None => Ok(snapshot),
                };
            }

            if !tasks.is_empty() {
                if let Some(result) =
                    tokio::time::timeout(Duration::from_millis(500), tasks.join_next())
                        .await
                        .ok()
                        .flatten()
                {
                    match result {
                        Ok((poll, result)) => {
                            if poll {
                                poll_count = poll_count.saturating_sub(1);
                            } else {
                                create_count = create_count.saturating_sub(1);
                            }
                            if let Err(error) = result {
                                keep_first_error(&mut worker_error, error);
                                stopping.store(true, Ordering::Relaxed);
                            }
                        }
                        Err(error) => {
                            keep_first_error(&mut worker_error, Error::Storage(error.to_string()));
                            stopping.store(true, Ordering::Relaxed);
                        }
                    }
                    if tasks.is_empty() {
                        create_count = 0;
                        poll_count = 0;
                    }
                }
            } else if stopping.load(Ordering::Relaxed) {
                return match worker_error {
                    Some(error) => Err(error),
                    None => Ok(snapshot),
                };
            } else {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    async fn run_task(
        self: &Arc<Self>,
        workspace: &Workspace,
        job: &UploadJob,
        poll: bool,
        key: &str,
    ) -> Result<(), Error> {
        let credential_loader = self.credential_loader;
        let credentials = tokio::task::spawn_blocking(credential_loader)
            .await
            .map_err(|error| Error::Storage(error.to_string()))?;
        let credentials = match credentials {
            Ok(Some(credentials)) if credentials.account_id == job.account_id => credentials,
            _ => {
                let db = workspace.clone();
                let job = job.clone();
                tokio::task::spawn_blocking(move || {
                    let _ = db.upload_defer(
                        &job,
                        if poll { "processing" } else { "queued" },
                        0,
                        "Configure the queued Roblox account, then resume this batch.",
                    );
                    db.pause_upload_account(&job.account_id)
                })
                .await
                .map_err(|error| Error::Storage(error.to_string()))??;
                return Ok(());
            }
        };
        self.execute(workspace, job, poll, key, &credentials, false)
            .await;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        sync::atomic::AtomicUsize,
        time::Duration,
    };

    fn mock_credentials() -> Result<Option<RobloxCredentials>, Error> {
        Ok(Some(RobloxCredentials {
            account_id: "123".into(),
            api_key: "test-only".to_owned().into(),
        }))
    }

    fn test_service(base_url: &str) -> RobloxUploadService {
        RobloxUploadService {
            http: Client::new(),
            base_url: base_url.into(),
            credential_loader: mock_credentials,
            budgets: Mutex::new(HashMap::new()),
            concurrency: AtomicUsize::new(5),
            clean_since: Mutex::new(queue_now()),
            last_transfer: AtomicI64::new(0),
        }
    }

    fn upload_fixture() -> (tempfile::TempDir, Workspace, String) {
        let directory = tempfile::tempdir().expect("temporary workspace");
        let workspace =
            Workspace::open(directory.path().join("workspace")).expect("open workspace");
        let project = workspace
            .save_project(
                None,
                apex_core::ProjectInput {
                    name: "HTTP queue test".into(),
                    ..Default::default()
                },
            )
            .expect("create project");
        let vehicle = apex_core::vehicle_catalogue()
            .expect("vehicle catalogue")
            .vehicles
            .remove(0);
        let source = directory.path().join("Left1.png");
        std::fs::write(
            &source,
            include_bytes!("../../../apps/erlcx-extension/icon32.png"),
        )
        .expect("write mock texture");
        let folder = workspace
            .import_livery(
                &project.id,
                None,
                None,
                "Test",
                &vehicle.id,
                &[(source, "Left1.png".into(), Some("Left 1".into()))],
                None,
            )
            .expect("import livery");
        workspace
            .create_upload_batch("batch", "123", std::slice::from_ref(&folder), false)
            .expect("create upload batch");
        (directory, workspace, folder)
    }

    fn read_request(socket: &mut std::net::TcpStream) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let count = socket.read(&mut chunk).expect("read mock request");
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        bytes
    }

    fn respond(socket: &mut std::net::TcpStream, status: u16, body: &str) {
        write!(
            socket,
            "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nRetry-After: 3\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("write mock response");
    }

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

    #[test]
    fn rate_budget_ramps_only_with_server_budget_and_honors_retry_after() {
        let service = test_service("");
        let now = queue_now();
        for index in 1..=12 {
            let tick = now + index * 30_001;
            service.last_transfer.store(tick, Ordering::Relaxed);
            service.ramp(tick);
        }
        assert_eq!(service.concurrency(), 5);

        service.budgets.lock().expect("rate budgets").insert(
            "123:create".into(),
            RateBudget {
                remaining: Some(1000),
                ..Default::default()
            },
        );
        for index in 13..=25 {
            let tick = now + index * 30_001;
            service.last_transfer.store(tick, Ordering::Relaxed);
            service.ramp(tick);
        }
        assert_eq!(service.concurrency(), 10);

        service.reset_concurrency();
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "2".parse().expect("retry header"));
        headers.insert("x-ratelimit-reset", "4".parse().expect("reset header"));
        let before = queue_now();
        assert!(service.observe_headers("123:create", 429, &headers) >= before + 4000);
        assert_eq!(service.concurrency(), 2);
        assert_eq!(service.blocked("456:create"), 0);
    }

    #[test]
    fn queue_service_runs_two_upload_passes_and_selects_the_second_decal_id() {
        let (_directory, workspace, folder) = upload_fixture();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("mock listener");
        let base = format!("http://{}", listener.local_addr().expect("mock address"));
        let server = std::thread::spawn(move || {
            for (method, response) in [
                ("POST", r#"{"path":"operations/one"}"#),
                (
                    "GET",
                    r#"{"done":true,"response":{"assetType":"Decal","assetId":"111","creationContext":{"creator":{"userId":"123"}}}}"#,
                ),
                ("POST", r#"{"path":"operations/two"}"#),
                (
                    "GET",
                    r#"{"done":true,"response":{"assetType":"Decal","assetId":"222","creationContext":{"creator":{"userId":"123"}}}}"#,
                ),
            ] {
                let (mut socket, _) = listener.accept().expect("mock connection");
                let request = read_request(&mut socket);
                assert!(request.starts_with(method.as_bytes()));
                if method == "POST" {
                    let png = include_bytes!("../../../apps/erlcx-extension/icon32.png");
                    assert!(request.windows(png.len()).any(|window| window == png));
                }
                respond(&mut socket, 200, response);
            }
        });
        let service = test_service(&base);
        let credentials = RobloxCredentials {
            account_id: "123".into(),
            api_key: "test-only".to_owned().into(),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        runtime.block_on(async {
            for pass in 1..=2 {
                let job = workspace
                    .lease_upload_tasks(false, 10)
                    .expect("lease upload")
                    .remove(0);
                assert_eq!(job.pass, pass);
                service
                    .execute(&workspace, &job, false, "123:create", &credentials, false)
                    .await;
                assert!(workspace
                    .lease_upload_tasks(false, 10)
                    .expect("check create queue")
                    .is_empty());
                let job = workspace
                    .upload_snapshot()
                    .expect("queue snapshot")
                    .jobs
                    .remove(0);
                workspace
                    .upload_defer(&job, "processing", 0, "")
                    .expect("make operation pollable");
                let job = workspace
                    .lease_upload_tasks(true, 2)
                    .expect("lease operation poll")
                    .remove(0);
                service
                    .execute(&workspace, &job, true, "123:poll", &credentials, false)
                    .await;
            }
        });
        server.join().expect("mock Roblox server");
        assert_eq!(
            workspace
                .livery_uploads(&folder, "123")
                .expect("livery upload state")
                .textures[0]
                .upload
                .as_ref()
                .expect("selected upload")
                .asset_id
                .as_deref(),
            Some("222")
        );
    }

    #[test]
    fn authentication_pauses_and_uncertain_post_requires_review() {
        for status in [401, 429, 500] {
            let (_directory, workspace, _) = upload_fixture();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("mock listener");
            let base = format!("http://{}", listener.local_addr().expect("mock address"));
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().expect("mock request");
                read_request(&mut socket);
                respond(&mut socket, status, "{}");
            });
            let job = workspace
                .lease_upload_tasks(false, 1)
                .expect("lease upload")
                .remove(0);
            let credentials = RobloxCredentials {
                account_id: "123".into(),
                api_key: "test-only".to_owned().into(),
            };
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime");
            runtime.block_on(test_service(&base).execute(
                &workspace,
                &job,
                false,
                "123:create",
                &credentials,
                false,
            ));
            server.join().expect("mock Roblox server");
            let snapshot = workspace.upload_snapshot().expect("queue snapshot");
            assert_eq!(
                snapshot.jobs[0].state,
                if status == 500 {
                    "needs_review"
                } else {
                    "queued"
                }
            );
            if status == 401 {
                assert_eq!(snapshot.batches[0].state, "paused");
            }
            if status == 429 {
                assert!(snapshot.jobs[0].next_at > queue_now() + 2000);
            }
        }
    }

    #[test]
    fn queue_runner_returns_without_credentials_when_no_batches_are_running() {
        let directory = tempfile::tempdir().expect("temporary workspace");
        let workspace =
            Workspace::open(directory.path().join("workspace")).expect("open workspace");
        let service = Arc::new(test_service(""));
        let stopping = Arc::new(AtomicBool::new(false));
        let updates = Arc::new(AtomicUsize::new(0));
        let callback_updates = updates.clone();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let snapshot = runtime
            .block_on(
                service.run_until_idle(workspace, stopping, move |snapshot, active| {
                    assert!(snapshot.batches.is_empty());
                    assert_eq!(active, 0);
                    callback_updates.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .expect("empty queue result");
        assert!(snapshot.batches.is_empty());
        assert_eq!(updates.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn queue_runner_processes_a_dummy_two_pass_batch_without_the_desktop_adapter() {
        let (_directory, workspace, folder) = upload_fixture();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("mock listener");
        let base = format!("http://{}", listener.local_addr().expect("mock address"));
        let server = std::thread::spawn(move || {
            for (method, response) in [
                ("POST", r#"{"path":"operations/one"}"#),
                (
                    "GET",
                    r#"{"done":true,"response":{"assetType":"Decal","assetId":"111","creationContext":{"creator":{"userId":"123"}}}}"#,
                ),
                ("POST", r#"{"path":"operations/two"}"#),
                (
                    "GET",
                    r#"{"done":true,"response":{"assetType":"Decal","assetId":"222","creationContext":{"creator":{"userId":"123"}}}}"#,
                ),
            ] {
                let (mut socket, _) = listener.accept().expect("mock connection");
                let request = read_request(&mut socket);
                assert!(request.starts_with(method.as_bytes()));
                if method == "POST" {
                    let png = include_bytes!("../../../apps/erlcx-extension/icon32.png");
                    assert!(request.windows(png.len()).any(|window| window == png));
                }
                respond(&mut socket, 200, response);
            }
        });
        let service = Arc::new(test_service(&base));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let snapshot = runtime
            .block_on(service.run_until_idle(
                workspace.clone(),
                Arc::new(AtomicBool::new(false)),
                |_, _| {},
            ))
            .expect("run complete dummy batch");
        server.join().expect("mock Roblox server");
        assert_eq!(snapshot.batches[0].state, "complete");
        assert_eq!(
            workspace
                .livery_uploads(&folder, "123")
                .expect("livery upload state")
                .textures[0]
                .upload
                .as_ref()
                .expect("selected upload")
                .asset_id
                .as_deref(),
            Some("222")
        );
    }
}
