use super::{projects::work, roblox, CommandError};
use apex_core::Error;
use apex_storage::{queue_now, QueueSnapshot, RobloxLiverySummary, UploadJob, Workspace};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};
pub struct UploadRuntime {
    service: apex_application::roblox_upload::RobloxUploadService,
    posts: AtomicUsize,
    polls: AtomicUsize,
    pub editing: AtomicBool,
    pub stopping: AtomicBool,
    active: AtomicBool,
    dirty: AtomicBool,
    changing: AtomicBool,
    revision: std::sync::atomic::AtomicU64,
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
            service: apex_application::roblox_upload::RobloxUploadService::new()?,
            posts: AtomicUsize::new(0),
            polls: AtomicUsize::new(0),
            editing: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            active: AtomicBool::new(false),
            dirty: AtomicBool::new(true),
            changing: AtomicBool::new(false),
            revision: std::sync::atomic::AtomicU64::new(0),
        }))
    }
    pub fn snapshot(&self, ws: &Workspace) -> Result<UploadSnapshot, Error> {
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(UploadSnapshot {
            revision,
            queue: ws.upload_snapshot()?,
            concurrency: self.service.concurrency(),
            active_transfers: self.posts.load(Ordering::Relaxed),
            next_allowed_at: self.service.next_allowed_at(),
        })
    }
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }
    fn ramp(&self, now: i64) {
        self.service.ramp(now);
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
                        service.service.concurrency()
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
                        let until = service.service.reserve(&key);
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
        self.service
            .execute(
                ws,
                job,
                poll,
                key,
                &c,
                self.stopping.load(Ordering::Relaxed),
            )
            .await;
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
        runtime.service.reset_concurrency();
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
    double_upload: bool,
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
        let double_upload = ws.roblox_double_upload()?;
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
            creates: additional * if double_upload { 2 } else { 1 },
            double_upload,
        })
    })
    .await
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
