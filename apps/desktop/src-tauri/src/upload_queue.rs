use super::{projects::work, roblox, CommandError};
use apex_core::Error;
use apex_storage::{queue_now, QueueSnapshot, RobloxLiverySummary, UploadJob, Workspace};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};
#[derive(Default)]
struct Budget {
    until: i64,
    remaining: Option<i64>,
    failures: u32,
    reset_at: i64,
}
pub struct UploadRuntime {
    http: reqwest::Client,
    posts: AtomicUsize,
    polls: AtomicUsize,
    concurrency: AtomicUsize,
    budgets: Mutex<HashMap<String, Budget>>,
    clean_since: Mutex<i64>,
    pub editing: AtomicBool,
    pub stopping: AtomicBool,
    active: AtomicBool,
    dirty: AtomicBool,
    changing: AtomicBool,
    revision: std::sync::atomic::AtomicU64,
    last_transfer: std::sync::atomic::AtomicI64,
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UploadSnapshot {
    pub queue: QueueSnapshot,
    pub revision: u64,
    pub concurrency: usize,
    pub active_transfers: usize,
    pub next_allowed_at: i64,
}
impl UploadRuntime {
    pub fn changed(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }
    pub fn new() -> Result<Arc<Self>, Error> {
        Ok(Arc::new(Self {
            http: roblox::client()?,
            posts: AtomicUsize::new(0),
            polls: AtomicUsize::new(0),
            concurrency: AtomicUsize::new(5),
            budgets: Mutex::new(HashMap::new()),
            clean_since: Mutex::new(queue_now()),
            editing: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            active: AtomicBool::new(false),
            dirty: AtomicBool::new(true),
            changing: AtomicBool::new(false),
            revision: std::sync::atomic::AtomicU64::new(0),
            last_transfer: std::sync::atomic::AtomicI64::new(0),
        }))
    }
    #[cfg(test)]
    fn blocked(&self, key: &str) -> i64 {
        self.budgets
            .lock()
            .ok()
            .and_then(|m| m.get(key).map(|b| b.until))
            .unwrap_or(0)
    }
    fn reserve(&self, key: &str) -> i64 {
        let now = queue_now();
        let Ok(mut map) = self.budgets.lock() else {
            return now + 1000;
        };
        let b = map.entry(key.into()).or_default();
        if b.until > now {
            return b.until;
        }
        if b.remaining == Some(0) {
            if b.reset_at > now {
                b.until = b.reset_at;
                return b.reset_at;
            }
            b.remaining = None;
        }
        if let Some(n) = b.remaining.as_mut() {
            *n = (*n - 1).max(0);
        }
        0
    }
    fn observe(&self, key: &str, r: &reqwest::Response) -> i64 {
        self.observe_headers(key, r.status().as_u16(), r.headers())
    }
    fn observe_headers(&self, key: &str, status: u16, h: &reqwest::header::HeaderMap) -> i64 {
        let now = queue_now();
        if (200..300).contains(&status) {
            self.last_transfer.store(now, Ordering::Relaxed);
        }
        let mut until = 0;
        if let Ok(mut budgets) = self.budgets.lock() {
            let b = budgets.entry(key.into()).or_default();
            let seconds = |name: &str| {
                h.get(name)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.trim().parse::<i64>().ok())
            };
            let remaining = seconds("x-ratelimit-remaining").map(|n| n.max(0));
            b.remaining = if b.reset_at > now {
                match (b.remaining, remaining) {
                    (Some(a), Some(n)) => Some(a.min(n)),
                    (a, None) => a,
                    (_, n) => n,
                }
            } else {
                remaining
            };
            if remaining.is_some() {
                b.reset_at =
                    now + seconds("x-ratelimit-reset").unwrap_or(30).clamp(0, 86400) * 1000;
            }
            if status == 429 {
                b.failures = (b.failures + 1).min(6);
                let retry = h
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| {
                        s.parse::<i64>()
                            .ok()
                            .map(|n| n.clamp(0, 86400) * 1000)
                            .or_else(|| {
                                httpdate::parse_http_date(s)
                                    .ok()
                                    .and_then(|d| {
                                        d.duration_since(std::time::SystemTime::now()).ok()
                                    })
                                    .map(|d| d.as_millis() as i64)
                            })
                    })
                    .unwrap_or((1i64 << b.failures) * 1000);
                let reset = seconds("x-ratelimit-reset").unwrap_or(0).clamp(0, 86400) * 1000;
                b.until = now + retry.max(reset) + now.rem_euclid(401) + 100;
                self.concurrency.store(
                    (self.concurrency.load(Ordering::Relaxed) / 2).max(1),
                    Ordering::Relaxed,
                );
            } else if b.remaining == Some(0) {
                b.until = now + seconds("x-ratelimit-reset").unwrap_or(30).clamp(0, 86400) * 1000;
            } else if (200..300).contains(&status) {
                b.failures = 0;
            }
            until = b.until;
        }
        if status == 429 {
            if let Ok(mut clean) = self.clean_since.lock() {
                *clean = now;
            }
        }
        until
    }
    pub fn snapshot(&self, ws: &Workspace) -> Result<UploadSnapshot, Error> {
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(UploadSnapshot {
            revision,
            queue: ws.upload_snapshot()?,
            concurrency: self.concurrency.load(Ordering::Relaxed),
            active_transfers: self.posts.load(Ordering::Relaxed),
            next_allowed_at: self
                .budgets
                .lock()
                .map_err(|_| Error::Storage("Upload rate state unavailable.".into()))?
                .values()
                .map(|b| b.until)
                .max()
                .unwrap_or(0),
        })
    }
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }
    fn ramp(&self, now: i64) {
        if let Ok(mut since) = self.clean_since.lock() {
            if now - self.last_transfer.load(Ordering::Relaxed) < 2000
                && now - *since >= 30_000
                && self
                    .budgets
                    .lock()
                    .map(|m| {
                        m.values().any(|b| b.remaining.unwrap_or(0) > 10)
                            && m.values().all(|b| b.until <= now)
                    })
                    .unwrap_or(false)
            {
                self.concurrency
                    .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                        Some((n + 1).min(10))
                    })
                    .ok();
                *since = now;
            }
        }
    }
    pub fn start(self: &Arc<Self>, app: tauri::AppHandle, ws: Workspace) {
        self.active.store(
            ws.upload_snapshot()
                .map(|s| {
                    s.batches
                        .iter()
                        .any(|b| ["running", "paused"].contains(&b.state.as_str()))
                })
                .unwrap_or(false),
            Ordering::Relaxed,
        );
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            while !service.stopping.load(Ordering::Relaxed) {
                if service.changing.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                service.ramp(queue_now());
                for poll in [true, false] {
                    let in_flight = if poll {
                        service.polls.load(Ordering::Relaxed)
                    } else {
                        service.posts.load(Ordering::Relaxed)
                    };
                    let cap = if poll {
                        2
                    } else {
                        service.concurrency.load(Ordering::Relaxed)
                    };
                    let count = cap.saturating_sub(in_flight);
                    if count == 0 {
                        continue;
                    }
                    let db = ws.clone();
                    let tasks = work(move || db.lease_upload_tasks(poll, count))
                        .await
                        .unwrap_or_default();
                    if !tasks.is_empty() {
                        service.dirty.store(true, Ordering::Relaxed);
                    }
                    for job in tasks {
                        let key = format!(
                            "{}:{}",
                            job.account_id,
                            if poll { "poll" } else { "create" }
                        );
                        let until = service.reserve(&key);
                        if until > queue_now() {
                            let _ = ws.upload_defer(
                                &job,
                                if poll { "processing" } else { "queued" },
                                until - queue_now(),
                                "Waiting for Roblox rate limit.",
                            );
                            continue;
                        }
                        if poll {
                            service.polls.fetch_add(1, Ordering::Relaxed);
                        } else {
                            service.posts.fetch_add(1, Ordering::Relaxed);
                        }
                        let service = service.clone();
                        let db = ws.clone();
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            service.process(&handle, &db, &job, poll, &key).await;
                            service.dirty.store(true, Ordering::Relaxed);
                            if poll {
                                service.polls.fetch_sub(1, Ordering::Relaxed);
                            } else {
                                service.posts.fetch_sub(1, Ordering::Relaxed);
                            }
                        });
                    }
                }
                if service.dirty.swap(false, Ordering::Relaxed) {
                    if let Ok(snapshot) = service.snapshot(&ws) {
                        service.active.store(
                            snapshot
                                .queue
                                .batches
                                .iter()
                                .any(|b| ["running", "paused"].contains(&b.state.as_str())),
                            Ordering::Relaxed,
                        );
                        let _ = app.emit("roblox-queue", snapshot);
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }
    async fn process(
        &self,
        app: &tauri::AppHandle,
        ws: &Workspace,
        job: &UploadJob,
        poll: bool,
        key: &str,
    ) {
        let credentials = {
            let state = app.state::<roblox::RobloxState>();
            let _guard = state.0.lock().await;
            work(roblox::required).await
        };
        let c = match credentials {
            Ok(c) if c.account_id == job.account_id => c,
            _ => {
                let _ = ws.upload_defer(
                    job,
                    if poll { "processing" } else { "queued" },
                    0,
                    "Configure the queued Roblox account in Settings, then Resume.",
                );
                let _ = ws.pause_upload_account(&job.account_id);
                return;
            }
        };
        self.execute(ws, job, poll, key, &c, "https://apis.roblox.com/assets/v1")
            .await;
    }
    async fn execute(
        &self,
        ws: &Workspace,
        job: &UploadJob,
        poll: bool,
        key: &str,
        c: &roblox::Credentials,
        base: &str,
    ) {
        if self.stopping.load(Ordering::Relaxed) {
            let _ = ws.upload_defer(
                job,
                if poll { "processing" } else { "queued" },
                0,
                "Paused while Apex Studio exits.",
            );
            return;
        }
        let request = if poll {
            let Some(path) = &job.operation else {
                let _ = ws.upload_defer(
                    job,
                    "needs_review",
                    0,
                    "Missing operation reference. Check Creator Dashboard.",
                );
                return;
            };
            if roblox::operation_path(&serde_json::json!({"path":path})).is_err() {
                let _ = ws.upload_defer(job, "needs_review", 0, "Invalid operation reference.");
                return;
            }
            self.http
                .get(format!("{base}/{path}"))
                .headers(match roblox::headers(c) {
                    Ok(h) => h,
                    Err(_) => return,
                })
                .send()
                .await
                .map_err(|_| ())
        } else {
            let db = ws.clone();
            let hash = job.hash.clone();
            let bytes = match work(move || db.decal_bytes(&hash)).await {
                Ok(b) => b,
                Err(e) => {
                    let _ = ws.upload_defer(job, "failed", 0, &e.message);
                    return;
                }
            };
            roblox::create_decal(&self.http, &format!("{base}/assets"), c, &job.name, bytes)
                .await
                .map_err(|_| ())
        };
        let response = match request {
            Ok(r) => r,
            Err(_) => {
                let _=ws.upload_defer(job,if poll{"processing"}else{"needs_review"},if poll{8000}else{0},if poll{"Connection interrupted; status will be checked again."}else{"Upload result unknown. Check Creator Dashboard; this pass will not be repeated automatically."});
                return;
            }
        };
        let status = response.status().as_u16();
        let until = self.observe(key, &response);
        if status == 429 {
            let _ = ws.upload_defer(
                job,
                if poll { "processing" } else { "queued" },
                (until - queue_now()).max(2000),
                "Waiting for Roblox rate limit.",
            );
            return;
        }
        if status == 401 || status == 403 {
            let _ = ws.upload_defer(
                job,
                if poll { "processing" } else { "queued" },
                0,
                "Roblox rejected the key or its permissions. Verify Settings and Resume.",
            );
            let _ = ws.pause_upload_account(&job.account_id);
            return;
        }
        if !(200..300).contains(&status) {
            let uncertain = !poll && (status >= 500 || status == 408);
            let _=ws.upload_defer(job,if uncertain{"needs_review"}else if poll&&status>=500{"processing"}else{"failed"},8000,"Roblox rejected or could not complete this request. Review the texture and retry if appropriate.");
            return;
        }
        let value = match roblox::body(response).await {
            Ok(v) => v,
            Err(_) => {
                let _ = ws.upload_defer(
                    job,
                    if poll { "processing" } else { "needs_review" },
                    8000,
                    "Roblox response was interrupted. Check the saved operation before retrying.",
                );
                return;
            }
        };
        if !poll {
            match roblox::operation_path(&value) {
                Ok(path) => {
                    let _ = ws.upload_operation(job, &path);
                }
                Err(_) => {
                    let _ = ws.upload_defer(
                        job,
                        "needs_review",
                        0,
                        "Roblox did not return an operation. Check Creator Dashboard.",
                    );
                }
            }
            return;
        }
        if value.get("error").is_some_and(|v| !v.is_null()) {
            let _ = ws.fail_upload_operation(job);
            return;
        }
        if value["done"] != true {
            let _ = ws.upload_defer(
                job,
                "processing",
                [2000, 4000, 8000, 15000][(job.poll_attempts as usize).min(3)],
                "Roblox is processing this upload.",
            );
            return;
        }
        let mut result = apex_storage::DecalUpload {
            account_id: job.account_id.clone(),
            hash: job.hash.clone(),
            state: String::new(),
            operation: job.operation.clone(),
            asset_id: None,
            moderation: "unknown".into(),
            message: String::new(),
        };
        match roblox::apply_operation(&mut result, &value) {
            Ok(()) => {
                if let Some(id) = result.asset_id {
                    let _ = ws.upload_pass_complete(job, &id, &result.moderation);
                }
            }
            Err(e) => {
                let _ = ws.upload_defer(job, "needs_review", 0, &e.to_string());
            }
        }
    }
    pub async fn stop_for_credentials(&self, ws: &Workspace) -> CredentialChange<'_> {
        self.changing.store(true, Ordering::Relaxed);
        self.dirty.store(true, Ordering::Relaxed);
        let _ = ws.control_upload_batch(None, "pause");
        while self.posts.load(Ordering::Relaxed) + self.polls.load(Ordering::Relaxed) > 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        CredentialChange {
            runtime: self,
            ws: ws.clone(),
        }
    }
    pub fn exit(self: &Arc<Self>, app: tauri::AppHandle, ws: Workspace) {
        self.stopping.store(true, Ordering::Relaxed);
        let _ = ws.control_upload_batch(None, "pause");
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            for _ in 0..100 {
                if service.posts.load(Ordering::Relaxed) + service.polls.load(Ordering::Relaxed)
                    == 0
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            app.exit(0);
        });
    }
}
#[tauri::command]
pub async fn roblox_liveries(
    ws: tauri::State<'_, Workspace>,
) -> Result<Vec<RobloxLiverySummary>, CommandError> {
    let ws = ws.inner().clone();
    work(move || {
        let account = roblox::load()?
            .map(|c| c.account_id.clone())
            .unwrap_or_default();
        ws.roblox_liveries(&account)
    })
    .await
}
#[tauri::command]
pub async fn upload_snapshot(
    ws: tauri::State<'_, Workspace>,
    runtime: tauri::State<'_, Arc<UploadRuntime>>,
) -> Result<UploadSnapshot, CommandError> {
    let ws = ws.inner().clone();
    let runtime = runtime.inner().clone();
    work(move || runtime.snapshot(&ws)).await
}
#[tauri::command]
pub async fn start_upload_batch(
    request_id: String,
    folder_ids: Vec<String>,
    force: bool,
    ws: tauri::State<'_, Workspace>,
    runtime: tauri::State<'_, Arc<UploadRuntime>>,
) -> Result<String, CommandError> {
    let ws = ws.inner().clone();
    if runtime.stopping.load(Ordering::Relaxed) {
        return Err(Error::InvalidContent("Apex is closing.".into()).into());
    }
    if !runtime.is_active() {
        runtime.concurrency.store(5, Ordering::Relaxed);
        if let Ok(mut since) = runtime.clean_since.lock() {
            *since = queue_now();
        }
    }
    runtime.active.store(true, Ordering::Relaxed);
    runtime.dirty.store(true, Ordering::Relaxed);
    let result = work(move || {
        let c = roblox::required()?;
        ws.create_upload_batch(&request_id, &c.account_id, &folder_ids, force)
    })
    .await;
    runtime.dirty.store(true, Ordering::Relaxed);
    result
}
#[tauri::command]
pub async fn control_upload_batch(
    batch_id: Option<String>,
    action: String,
    runtime: tauri::State<'_, Arc<UploadRuntime>>,
    ws: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = ws.inner().clone();
    let result = work(move || ws.control_upload_batch(batch_id.as_deref(), &action)).await;
    runtime.dirty.store(true, Ordering::Relaxed);
    result
}
#[tauri::command]
pub fn set_edit_lock(locked: bool, runtime: tauri::State<'_, Arc<UploadRuntime>>) {
    runtime.editing.store(locked, Ordering::Relaxed);
}
pub fn setup_tray(
    app: &tauri::AppHandle,
    runtime: Arc<UploadRuntime>,
    ws: Workspace,
) -> tauri::Result<()> {
    use tauri::{
        image::Image,
        menu::{Menu, MenuItem},
        tray::{TrayIconBuilder, TrayIconEvent},
    };
    let open = MenuItem::with_id(app, "open", "Open Apex Studio", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause uploads", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume uploads", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &resume, &exit])?;
    let queue = runtime.clone();
    let db = ws.clone();
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    TrayIconBuilder::with_id("apex-uploads")
        .icon(icon)
        .tooltip("Apex Studio — Roblox uploads")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            "pause" => {
                let _ = db.control_upload_batch(None, "pause");
                queue.dirty.store(true, Ordering::Relaxed);
            }
            "resume" => {
                let _ = db.control_upload_batch(None, "resume");
                queue.dirty.store(true, Ordering::Relaxed);
            }
            "exit" => {
                if queue.editing.load(Ordering::Relaxed) {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    let _ = app.emit("close-blocked", ());
                } else {
                    queue.exit(app.clone(), db.clone());
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                if let Some(w) = tray.app_handle().get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadReview {
    account_id: String,
    liveries: usize,
    textures: usize,
    unique_textures: usize,
    reused: usize,
    queued: usize,
    creates: usize,
}
#[tauri::command]
pub async fn preview_upload_batch(
    folder_ids: Vec<String>,
    force: bool,
    ws: tauri::State<'_, Workspace>,
) -> Result<UploadReview, CommandError> {
    let ws = ws.inner().clone();
    work(move || {
        let c = roblox::required()?;
        if folder_ids.is_empty() || folder_ids.len() > 500 {
            return Err(Error::InvalidContent("Select up to 500 liveries.".into()));
        }
        let mut unique = std::collections::HashMap::new();
        let mut textures = 0;
        for id in &folder_ids {
            for t in ws.livery_uploads(id, &c.account_id)?.textures {
                textures += 1;
                unique.entry(t.hash).or_insert(t.upload);
            }
        }
        let reused = if force {
            0
        } else {
            unique
                .values()
                .filter(|u| {
                    u.as_ref().is_some_and(|u| {
                        u.state == "uploaded" && u.asset_id.is_some() && u.moderation != "rejected"
                    })
                })
                .count()
        };
        let active = ws
            .upload_snapshot()?
            .jobs
            .into_iter()
            .filter(|j| {
                j.account_id == c.account_id
                    && !["complete", "cancelled"].contains(&j.state.as_str())
            })
            .map(|j| j.hash)
            .collect::<std::collections::HashSet<_>>();
        let queued = unique.keys().filter(|h| active.contains(*h)).count();
        let additional = unique
            .iter()
            .filter(|(h, u)| {
                !active.contains(*h)
                    && (force
                        || !u.as_ref().is_some_and(|u| {
                            u.state == "uploaded"
                                && u.asset_id.is_some()
                                && u.moderation != "rejected"
                        }))
            })
            .count();
        Ok(UploadReview {
            account_id: c.account_id.clone(),
            liveries: folder_ids.len(),
            textures,
            unique_textures: unique.len(),
            reused,
            queued,
            creates: additional * 2,
        })
    })
    .await
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn clean_budget_ramps_to_ten_but_unknown_budgets_do_not() {
        let rt = UploadRuntime::new().unwrap();
        let now = queue_now();
        for i in 1..=12 {
            let tick = now + i * 30_001;
            rt.last_transfer.store(tick, Ordering::Relaxed);
            rt.ramp(tick);
        }
        assert_eq!(rt.concurrency.load(Ordering::Relaxed), 5);
        rt.budgets.lock().unwrap().insert(
            "123:create".into(),
            Budget {
                remaining: Some(1000),
                ..Default::default()
            },
        );
        for i in 13..=25 {
            let tick = now + i * 30_001;
            rt.last_transfer.store(tick, Ordering::Relaxed);
            rt.ramp(tick);
        }
        assert_eq!(rt.concurrency.load(Ordering::Relaxed), 10);
    }
    #[test]
    fn throttling_obeys_reset_and_retry_after_and_halves_concurrency() {
        let rt = UploadRuntime::new().unwrap();
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("retry-after", "2".parse().unwrap());
        h.insert("x-ratelimit-reset", "4".parse().unwrap());
        let before = queue_now();
        let until = rt.observe_headers("123:create", 429, &h);
        assert!(until >= before + 4000);
        assert_eq!(rt.concurrency.load(Ordering::Relaxed), 2);
        assert_eq!(rt.blocked("456:create"), 0);
        rt.observe_headers("123:create", 429, &h);
        assert_eq!(rt.concurrency.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn exhausted_success_budget_waits_and_http_date_is_supported() {
        let rt = UploadRuntime::new().unwrap();
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("x-ratelimit-remaining", "0".parse().unwrap());
        h.insert("x-ratelimit-reset", "20".parse().unwrap());
        assert!(rt.observe_headers("123:create", 200, &h) >= queue_now() + 19_900);
        let date = httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(60));
        h.clear();
        h.insert("retry-after", date.parse().unwrap());
        assert!(rt.observe_headers("123:poll", 429, &h) >= queue_now() + 58_000);
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod transport_tests {
    use super::*;
    use std::io::{Read, Write};
    fn fixture() -> (tempfile::TempDir, Workspace, String) {
        let d = tempfile::tempdir().unwrap();
        let ws = Workspace::open(d.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                apex_core::ProjectInput {
                    name: "HTTP queue test".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let v = apex_core::vehicle_catalogue().unwrap().vehicles.remove(0);
        let path = d.path().join("Left1.png");
        std::fs::write(&path, include_bytes!("../../../erlcx-extension/icon32.png")).unwrap();
        let f = ws
            .import_livery(
                &p.id,
                None,
                None,
                "Test",
                &v.id,
                &[(path, "Left1.png".into(), Some("Left 1".into()))],
                None,
            )
            .unwrap();
        ws.create_upload_batch("batch", "123", std::slice::from_ref(&f), false)
            .unwrap();
        (d, ws, f)
    }
    fn service() -> UploadRuntime {
        UploadRuntime {
            http: reqwest::Client::new(),
            posts: AtomicUsize::new(0),
            polls: AtomicUsize::new(0),
            concurrency: AtomicUsize::new(5),
            budgets: Mutex::new(HashMap::new()),
            clean_since: Mutex::new(queue_now()),
            editing: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            active: AtomicBool::new(true),
            dirty: AtomicBool::new(false),
            changing: AtomicBool::new(false),
            revision: std::sync::atomic::AtomicU64::new(0),
            last_transfer: std::sync::atomic::AtomicI64::new(0),
        }
    }
    fn read_request(socket: &mut std::net::TcpStream) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let n = socket.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..end]);
                let len = header
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|s| s.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + len {
                    break;
                }
            }
        }
        bytes
    }
    #[test]
    fn http_two_pass_flow_waits_for_processing_and_preserves_identical_png_bytes() {
        let (_d, ws, f) = fixture();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for (method, body) in [
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
                let (mut socket, _) = listener.accept().unwrap();
                let request = read_request(&mut socket);
                assert!(request.starts_with(method.as_bytes()));
                if method == "POST" {
                    let png = include_bytes!("../../../erlcx-extension/icon32.png");
                    assert!(request.windows(png.len()).any(|w| w == png));
                }
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
        });
        let rt = service();
        let c = roblox::Credentials {
            account_id: "123".into(),
            api_key: "test-only".into(),
        };
        tauri::async_runtime::block_on(async {
            for pass in 1..=2 {
                let j = ws.lease_upload_tasks(false, 10).unwrap().remove(0);
                assert_eq!(j.pass, pass);
                rt.execute(&ws, &j, false, "123:create", &c, &base).await;
                assert!(ws.lease_upload_tasks(false, 10).unwrap().is_empty());
                let j = ws.upload_snapshot().unwrap().jobs.remove(0);
                ws.upload_defer(&j, "processing", 0, "").unwrap();
                let j = ws.lease_upload_tasks(true, 2).unwrap().remove(0);
                rt.execute(&ws, &j, true, "123:poll", &c, &base).await;
            }
        });
        server.join().unwrap();
        assert_eq!(
            ws.livery_uploads(&f, "123").unwrap().textures[0]
                .upload
                .as_ref()
                .unwrap()
                .asset_id
                .as_deref(),
            Some("222")
        );
    }
    #[test]
    fn authentication_pauses_and_rate_limit_defers_without_duplicate_post() {
        for status in [401, 429] {
            let (_d, ws, _) = fixture();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                read_request(&mut socket);
                write!(socket,"HTTP/1.1 {status} Error\r\nContent-Length: 2\r\nRetry-After: 3\r\nConnection: close\r\n\r\n{{}}").unwrap();
            });
            let j = ws.lease_upload_tasks(false, 1).unwrap().remove(0);
            let rt = service();
            let c = roblox::Credentials {
                account_id: "123".into(),
                api_key: "test-only".into(),
            };
            tauri::async_runtime::block_on(rt.execute(&ws, &j, false, "123:create", &c, &base));
            server.join().unwrap();
            let snapshot = ws.upload_snapshot().unwrap();
            assert_eq!(snapshot.jobs[0].state, "queued");
            if status == 401 {
                assert_eq!(snapshot.batches[0].state, "paused");
            } else {
                assert!(snapshot.jobs[0].next_at > queue_now() + 2000);
            }
            assert!(ws.lease_upload_tasks(false, 10).unwrap().is_empty());
        }
    }
    #[test]
    fn lost_post_response_is_held_for_review() {
        let (_d, ws, _) = fixture();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            read_request(&mut socket);
        });
        let j = ws.lease_upload_tasks(false, 1).unwrap().remove(0);
        let rt = service();
        let c = roblox::Credentials {
            account_id: "123".into(),
            api_key: "test-only".into(),
        };
        tauri::async_runtime::block_on(rt.execute(&ws, &j, false, "123:create", &c, &base));
        server.join().unwrap();
        assert_eq!(ws.upload_snapshot().unwrap().jobs[0].state, "needs_review");
        assert!(ws.lease_upload_tasks(false, 10).unwrap().is_empty());
    }
}

pub struct CredentialChange<'a> {
    runtime: &'a UploadRuntime,
    ws: Workspace,
}
impl Drop for CredentialChange<'_> {
    fn drop(&mut self) {
        let _ = self.ws.control_upload_batch(None, "pause");
        self.runtime.changing.store(false, Ordering::Relaxed);
        self.runtime.dirty.store(true, Ordering::Relaxed);
    }
}
