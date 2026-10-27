use super::{projects::work, CommandError};
use apex_core::{AssetBoard, AssetFolder, AssetVersion, Error};
use apex_storage::Workspace;
use std::{collections::HashMap, path::PathBuf, sync::Mutex};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
#[derive(Default)]
pub struct LiveryDrafts(Mutex<HashMap<String, Draft>>);
#[derive(Clone)]
enum Entry {
    Pending,
    Ready(apex_storage::PreparedTexture),
    Failed(String),
}
struct Preparation {
    entries: Mutex<Vec<Entry>>,
    wake: std::sync::Condvar,
    cancelled: std::sync::atomic::AtomicBool,
    last_event: std::sync::atomic::AtomicU64,
}
fn candidates(files: &[(PathBuf, String)]) -> Vec<Candidate> {
    files
        .iter()
        .enumerate()
        .map(|(index, (_, name))| Candidate {
            index,
            name: name.clone(),
            suggested_slot: apex_core::suggest_slot(name),
        })
        .collect()
}
fn preparation_executor() -> &'static std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>> {
    static EXEC: std::sync::OnceLock<std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>>> =
        std::sync::OnceLock::new();
    EXEC.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let rx = std::sync::Arc::new(Mutex::new(rx));
        for _ in 0..4 {
            let rx = rx.clone();
            std::thread::spawn(move || loop {
                let task = match rx.lock() {
                    Ok(r) => r.recv(),
                    Err(_) => return,
                };
                match task {
                    Ok(task) => task(),
                    Err(_) => return,
                }
            });
        }
        tx
    })
}
impl Preparation {
    fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            entries: Mutex::new(Vec::new()),
            wake: std::sync::Condvar::new(),
            cancelled: std::sync::atomic::AtomicBool::new(false),
            last_event: std::sync::atomic::AtomicU64::new(0),
        })
    }
    fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.wake.notify_all();
    }
    fn status(&self) -> apex_storage::PreparationProgress {
        let Ok(e) = self.entries.lock() else {
            return Default::default();
        };
        apex_storage::PreparationProgress {
            total: e.len(),
            ready: e.iter().filter(|e| matches!(e, Entry::Ready(_))).count(),
            errors: e
                .iter()
                .filter_map(|e| {
                    if let Entry::Failed(s) = e {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }
    fn add(
        self: &std::sync::Arc<Self>,
        ws: &Workspace,
        app: &tauri::AppHandle,
        token: &str,
        files: &[(PathBuf, String)],
    ) {
        use tauri::Emitter;
        let start = if let Ok(mut e) = self.entries.lock() {
            let start = e.len();
            e.extend(files.iter().map(|_| Entry::Pending));
            start
        } else {
            return;
        };
        for (i, (path, name)) in files.iter().enumerate() {
            let state = self.clone();
            let ws = ws.clone();
            let app = app.clone();
            let path = path.clone();
            let name = name.clone();
            let token = token.to_string();
            let _ = preparation_executor().send(Box::new(move || {
                if state.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                let result = ws.prepare_texture(&path);
                if let Ok(mut e) = state.entries.lock() {
                    e[start + i] = match result {
                        Ok(p) => Entry::Ready(p),
                        Err(e) => Entry::Failed(format!("{name}: {e}")),
                    };
                }
                state.wake.notify_all();
                let progress = state.status();
                let now = apex_storage::queue_now() as u64;
                let last = state.last_event.load(std::sync::atomic::Ordering::Relaxed);
                if (now.saturating_sub(last) >= 100
                    || progress.ready + progress.errors.len() == progress.total)
                    && state
                        .last_event
                        .compare_exchange(
                            last,
                            now,
                            std::sync::atomic::Ordering::Relaxed,
                            std::sync::atomic::Ordering::Relaxed,
                        )
                        .is_ok()
                {
                    let _ = app.emit(
                        "livery-preparation",
                        serde_json::json!({"token":token,"progress":progress}),
                    );
                }
            }));
        }
    }
    fn wait(&self) -> Result<Vec<apex_storage::PreparedTexture>, Error> {
        let mut e = self
            .entries
            .lock()
            .map_err(|_| Error::Storage("Preparation unavailable.".into()))?;
        while e.iter().any(|e| matches!(e, Entry::Pending)) {
            if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(Error::InvalidContent("Preparation cancelled.".into()));
            }
            e = self
                .wake
                .wait(e)
                .map_err(|_| Error::Storage("Preparation unavailable.".into()))?;
        }
        if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(Error::InvalidContent("Preparation cancelled.".into()));
        }
        e.iter()
            .map(|e| match e {
                Entry::Ready(p) => Ok(p.clone()),
                Entry::Failed(s) => Err(Error::InvalidContent(s.clone())),
                Entry::Pending => Err(Error::NotFound),
            })
            .collect()
    }
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationInfo {
    token: String,
    progress: apex_storage::PreparationProgress,
}
#[tauri::command]
pub async fn livery_preparations(
    tokens: Vec<String>,
    state: tauri::State<'_, LiveryDrafts>,
) -> Result<Vec<PreparationInfo>, CommandError> {
    if tokens.len() > 50 {
        return Err(Error::InvalidContent("Too many preparation tokens.".into()).into());
    }
    let drafts = state.0.lock().map_err(|_| CommandError {
        code: "unavailable".into(),
        message: "Preparation unavailable.".into(),
    })?;
    Ok(tokens
        .into_iter()
        .filter_map(|token| {
            drafts
                .get(&token)
                .and_then(|d| d.preparation.as_ref())
                .map(|p| PreparationInfo {
                    token,
                    progress: p.status(),
                })
        })
        .collect())
}
struct Draft {
    preparation: Option<std::sync::Arc<Preparation>>,
    project: String,
    _dir: tempfile::TempDir,
    files: Vec<(PathBuf, String)>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    index: usize,
    name: String,
    suggested_slot: Option<String>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftInfo {
    token: String,
    files: Vec<Candidate>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDraftInfo {
    name: String,
    draft: Option<DraftInfo>,
    warning: Option<String>,
    error: Option<String>,
}

#[cfg(test)]
fn stage_livery_folder(
    project: &str,
    source: &std::path::Path,
    remaining: &mut u64,
) -> Result<(Draft, Vec<Candidate>, usize), Error> {
    let metadata = std::fs::symlink_metadata(source).map_err(|e| Error::Storage(e.to_string()))?;
    if !metadata.file_type().is_dir() {
        return Err(Error::InvalidContent(
            "Choose a regular folder, not a shortcut or link.".into(),
        ));
    }
    let mut paths = Vec::new();
    let mut skipped = 0;
    for entry in std::fs::read_dir(source).map_err(|e| Error::Storage(e.to_string()))? {
        let entry = entry.map_err(|e| Error::Storage(e.to_string()))?;
        let kind = entry
            .file_type()
            .map_err(|e| Error::Storage(e.to_string()))?;
        if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png"))
        {
            paths.push(entry.path());
        } else {
            skipped += 1;
        }
        if paths.len() > 100 {
            return Err(Error::InvalidContent(
                "Use up to 100 PNG files per livery folder.".into(),
            ));
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err(Error::InvalidContent(
            "No PNG files directly inside this folder. Nested folders are not imported.".into(),
        ));
    }
    let dir = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let mut files = Vec::new();
    let mut candidates = Vec::new();
    let mut used = 0;
    for (index, path) in paths.into_iter().enumerate() {
        let name = path
            .file_name()
            .ok_or(Error::NotFound)?
            .to_string_lossy()
            .into_owned();
        let size = std::fs::metadata(&path)
            .map_err(|e| Error::Storage(e.to_string()))?
            .len();
        if size > 20 * 1024 * 1024 || used + size > *remaining {
            return Err(Error::InvalidContent(
                "Use PNG files up to 20 MB each and up to 2 GB across this batch.".into(),
            ));
        }
        let target = dir.path().join(format!("{index}.png"));
        std::fs::copy(&path, &target).map_err(|e| Error::Storage(e.to_string()))?;
        apex_storage::validate_texture(&target)?;
        used += size;
        candidates.push(Candidate {
            index,
            suggested_slot: apex_core::suggest_slot(&name),
            name: name.clone(),
        });
        files.push((target, name));
    }
    *remaining -= used;
    Ok((
        Draft {
            preparation: None,
            project: project.into(),
            _dir: dir,
            files,
        },
        candidates,
        skipped,
    ))
}

#[tauri::command]
pub async fn prepare_livery_folders(
    project_id: String,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<Vec<FolderDraftInfo>>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.editable_project(&project_id)?;
        let Some(selected) = app
            .dialog()
            .file()
            .set_title("Select livery folders — one livery per folder")
            .blocking_pick_folders()
        else {
            return Ok(None);
        };
        if selected.is_empty() || selected.len() > 50 {
            return Err(Error::InvalidContent(
                "Choose between 1 and 50 livery folders.".into(),
            ));
        }
        let mut prepared = Vec::new();
        let mut info = Vec::new();
        let mut remaining = 2 * 1024 * 1024 * 1024;
        for source in selected {
            let path = source
                .into_path()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let scanned = (|| -> Result<(Vec<(PathBuf, String)>, usize), Error> {
                if !std::fs::symlink_metadata(&path)
                    .map_err(|e| Error::Storage(e.to_string()))?
                    .file_type()
                    .is_dir()
                {
                    return Err(Error::InvalidContent("Choose a regular folder.".into()));
                }
                let mut files = Vec::new();
                let mut skipped = 0;
                for entry in std::fs::read_dir(path).map_err(|e| Error::Storage(e.to_string()))? {
                    let entry = entry.map_err(|e| Error::Storage(e.to_string()))?;
                    if entry
                        .file_type()
                        .map_err(|e| Error::Storage(e.to_string()))?
                        .is_file()
                        && entry
                            .path()
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                    {
                        let size = entry
                            .metadata()
                            .map_err(|e| Error::Storage(e.to_string()))?
                            .len();
                        if size > remaining {
                            return Err(Error::InvalidContent("Batch exceeds 2 GB.".into()));
                        }
                        remaining -= size;
                        files.push((
                            entry.path(),
                            entry.file_name().to_string_lossy().into_owned(),
                        ));
                    } else {
                        skipped += 1;
                    }
                }
                files.sort_by(|a, b| a.1.cmp(&b.1));
                if files.is_empty() || files.len() > 100 {
                    return Err(Error::InvalidContent(
                        "Use 1–100 PNGs directly inside each folder.".into(),
                    ));
                }
                Ok((files, skipped))
            })();
            match scanned {
                Ok((files, skipped)) => {
                    let token = uuid::Uuid::now_v7().to_string();
                    let preparation = Preparation::new();
                    preparation.add(&ws, &app, &token, &files);
                    let candidates = candidates(&files);
                    prepared.push((
                        token.clone(),
                        Draft {
                            preparation: Some(preparation),
                            project: project_id.clone(),
                            _dir: tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?,
                            files,
                        },
                    ));
                    info.push(FolderDraftInfo {
                        name,
                        draft: Some(DraftInfo {
                            token,
                            files: candidates,
                        }),
                        warning: (skipped > 0).then(|| {
                            format!("Skipped {skipped} non-PNG files, links or nested folders.")
                        }),
                        error: None,
                    });
                }
                Err(e) => info.push(FolderDraftInfo {
                    name,
                    draft: None,
                    warning: None,
                    error: Some(e.to_string()),
                }),
            }
        }
        let state = app.state::<LiveryDrafts>();
        let mut drafts = state
            .0
            .lock()
            .map_err(|_| Error::Storage("Livery selection unavailable.".into()))?;
        for d in drafts.values() {
            if let Some(p) = &d.preparation {
                p.cancel();
            }
        }
        drafts.clear();
        drafts.extend(prepared);
        Ok(Some(info))
    })
    .await
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assignment {
    index: usize,
    slot: Option<String>,
}
#[tauri::command]
pub async fn asset_board(
    project_id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<AssetBoard, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.board(&project_id)).await
}
#[tauri::command]
pub async fn save_folder(
    project_id: String,
    id: Option<String>,
    parent_id: Option<String>,
    name: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<AssetFolder, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.save_folder(&project_id, id.as_deref(), parent_id.as_deref(), &name)).await
}
#[tauri::command]
pub async fn move_asset(
    id: String,
    folder_id: Option<String>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.move_asset(&id, folder_id.as_deref())).await
}
#[tauri::command]
pub async fn asset_versions(
    id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<AssetVersion>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.versions(&id)).await
}
#[tauri::command]
pub async fn restore_asset_version(
    id: String,
    version_id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.restore_version(&id, &version_id)).await
}
#[tauri::command]
pub async fn set_folder_cover(
    id: String,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<bool, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("PNG image", &["png"])
            .set_title("Choose folder cover")
            .blocking_pick_file()
        else {
            return Ok(false);
        };
        ws.set_cover(
            &id,
            &file
                .into_path()
                .map_err(|e| Error::Storage(e.to_string()))?,
        )?;
        Ok(true)
    })
    .await
}
#[tauri::command]
pub async fn prepare_livery(
    project_id: String,
    token: Option<String>,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<DraftInfo>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.editable_project(&project_id)?;
        let Some(selected) = app
            .dialog()
            .file()
            .add_filter("PNG textures", &["png"])
            .set_title("Import livery textures")
            .blocking_pick_files()
        else {
            return Ok(None);
        };
        if selected.is_empty() || selected.len() > 100 {
            return Err(Error::InvalidContent(
                "Choose up to 100 PNG textures.".into(),
            ));
        }
        let mut files = Vec::new();
        for source in selected {
            let path = source
                .into_path()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let name = path
                .file_name()
                .ok_or(Error::NotFound)?
                .to_string_lossy()
                .into_owned();
            files.push((path, name));
        }
        let state = app.state::<LiveryDrafts>();
        let mut drafts = state
            .0
            .lock()
            .map_err(|_| Error::Storage("Livery selection unavailable.".into()))?;
        if let Some(token) = token {
            let draft = drafts.get_mut(&token).ok_or(Error::NotFound)?;
            if draft.project != project_id || draft.files.len() + files.len() > 100 {
                return Err(Error::InvalidContent("Use up to 100 PNGs.".into()));
            }
            if let Some(p) = &draft.preparation {
                p.add(&ws, &app, &token, &files);
            }
            draft.files.extend(files);
            return Ok(Some(DraftInfo {
                token,
                files: candidates(&draft.files),
            }));
        }
        let token = uuid::Uuid::now_v7().to_string();
        let info = candidates(&files);
        let preparation = Preparation::new();
        preparation.add(&ws, &app, &token, &files);
        for draft in drafts.values() {
            if let Some(p) = &draft.preparation {
                p.cancel();
            }
        }
        drafts.clear();
        drafts.insert(
            token.clone(),
            Draft {
                preparation: Some(preparation),
                project: project_id,
                _dir: tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?,
                files,
            },
        );
        Ok(Some(DraftInfo { token, files: info }))
    })
    .await
}
#[tauri::command]
#[expect(
    clippy::too_many_arguments,
    reason = "Typed IPC fields describe a single reviewed batch; paths remain in native draft state."
)]
pub async fn commit_livery(
    token: String,
    parent_id: Option<String>,
    existing_id: Option<String>,
    name: String,
    vehicle_id: String,
    assignments: Vec<Assignment>,
    cover_index: Option<usize>,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<String, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        let state = app.state::<LiveryDrafts>();
        let drafts = state
            .0
            .lock()
            .map_err(|_| Error::Storage("Livery selection unavailable.".into()))?;
        let draft = drafts.get(&token).ok_or(Error::NotFound)?;
        let project = draft.project.clone();
        let preparation = draft.preparation.clone();
        let draft_files = draft.files.clone();
        drop(drafts);
        if assignments.len() != draft_files.len() {
            return Err(Error::InvalidContent(
                "Review every selected texture.".into(),
            ));
        }
        let mut assignments = assignments;
        assignments.sort_by_key(|a| a.index);
        let mut files = Vec::new();
        for (i, a) in assignments.into_iter().enumerate() {
            if a.index != i {
                return Err(Error::InvalidContent("Invalid file assignments.".into()));
            }
            let (path, name) = &draft_files[i];
            files.push((path.clone(), name.clone(), a.slot));
        }
        let id = if let Some(p) = &preparation {
            let prepared = p.wait()?;
            let files = files
                .iter()
                .enumerate()
                .map(|(i, (_, name, slot))| (prepared[i].clone(), name.clone(), slot.clone()))
                .collect::<Vec<_>>();
            ws.import_prepared_livery(
                &project,
                parent_id.as_deref(),
                existing_id.as_deref(),
                &name,
                &vehicle_id,
                &files,
                cover_index,
            )?
        } else {
            ws.import_livery(
                &project,
                parent_id.as_deref(),
                existing_id.as_deref(),
                &name,
                &vehicle_id,
                &files,
                cover_index,
            )?
        };
        state
            .0
            .lock()
            .map_err(|_| Error::Storage("Livery selection unavailable.".into()))?
            .remove(&token);
        Ok(id)
    })
    .await
}
#[tauri::command]
pub async fn cancel_livery(
    token: String,
    state: tauri::State<'_, LiveryDrafts>,
) -> Result<(), CommandError> {
    if let Some(d) = state
        .0
        .lock()
        .map_err(|_| CommandError {
            code: "draft_unavailable".into(),
            message: "Livery selection unavailable.".into(),
        })?
        .remove(&token)
    {
        if let Some(p) = d.preparation {
            p.cancel();
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod folder_tests {
    use super::*;
    #[test]
    fn folder_batches_keep_same_named_files_separate_and_preserve_originals() {
        let root = tempfile::tempdir().unwrap();
        let mut remaining = 2 * 1024 * 1024 * 1024;
        let ws = Workspace::open(root.path().join("workspace")).unwrap();
        let project = ws
            .save_project(
                None,
                apex_core::ProjectInput {
                    name: "Batch".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let vehicle = apex_core::vehicle_catalogue().unwrap().vehicles.remove(0);
        for (i, name) in ["Patrol A", "Patrol B"].iter().enumerate() {
            let folder = root.path().join(name);
            std::fs::create_dir(&folder).unwrap();
            let texture = folder.join("Patrol Lft 1 9.PNG");
            // Reuse a valid bundled PNG without adding an image dependency to the adapter.
            std::fs::write(
                &texture,
                include_bytes!("../../../erlcx-extension/icon32.png"),
            )
            .unwrap();
            std::fs::write(folder.join("source.txt"), b"preserve me").unwrap();
            std::fs::create_dir(folder.join("nested")).unwrap();
            let (draft, candidates, skipped) =
                stage_livery_folder(&project.id, &folder, &mut remaining).unwrap();
            assert_eq!(skipped, 2);
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].suggested_slot.as_deref(), Some("Left 1"));
            let files: Vec<_> = draft
                .files
                .iter()
                .map(|(path, name)| (path.clone(), name.clone(), Some("Left 1".into())))
                .collect();
            let id = ws
                .import_livery(&project.id, None, None, name, &vehicle.id, &files, Some(0))
                .unwrap();
            assert_eq!(ws.folder(&id).unwrap().name, *name);
            assert_eq!(ws.board(&project.id).unwrap().assets.len(), i + 1);
            assert_eq!(
                std::fs::read(&texture).unwrap(),
                std::fs::read(&draft.files[0].0).unwrap()
            );
        }
        assert_eq!(ws.folders(&project.id).unwrap().len(), 2);
    }
    #[test]
    fn empty_invalid_and_oversized_batches_do_not_leave_staged_drafts() {
        let root = tempfile::tempdir().unwrap();
        let mut budget = 100;
        assert!(stage_livery_folder("p", root.path(), &mut budget).is_err());
        std::fs::write(root.path().join("bad.png"), b"not a PNG").unwrap();
        assert!(stage_livery_folder("p", root.path(), &mut budget).is_err());
        assert_eq!(budget, 100);
        std::fs::write(
            root.path().join("bad.png"),
            include_bytes!("../../../erlcx-extension/icon32.png"),
        )
        .unwrap();
        assert!(stage_livery_folder("p", root.path(), &mut budget).is_err());
        assert_eq!(budget, 100);
    }
}

#[tauri::command]
pub async fn assign_asset_slot(
    id: String,
    slot: Option<String>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.assign_slot(&id, slot.as_deref())).await
}
#[tauri::command]
pub async fn copy_asset_item(
    project_id: String,
    id: String,
    is_folder: bool,
    folder_id: Option<String>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.copy_item(&project_id, &id, is_folder, folder_id.as_deref())).await
}
