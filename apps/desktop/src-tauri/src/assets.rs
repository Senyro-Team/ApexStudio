use super::{projects::work, CommandError};
use apex_core::{AssetBoard, AssetFolder, AssetVersion, Error};
use apex_storage::Workspace;
use std::{collections::HashMap, path::PathBuf, sync::Mutex};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
#[derive(Default)]
pub struct LiveryDrafts(Mutex<HashMap<String, Draft>>);
struct Draft {
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
        let dir = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
        let mut files = Vec::new();
        let mut info = Vec::new();
        for (i, source) in selected.into_iter().enumerate() {
            let source = source
                .into_path()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let name = source
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .ok_or(Error::NotFound)?;
            let target = dir.path().join(format!("{i}.png"));
            let size = std::fs::metadata(&source)
                .map_err(|e| Error::Storage(e.to_string()))?
                .len();
            if size > 20 * 1024 * 1024 {
                return Err(Error::InvalidContent(format!(
                    "{name}: choose a PNG smaller than 20 MB."
                )));
            }
            std::fs::copy(&source, &target).map_err(|e| Error::Storage(e.to_string()))?;
            apex_storage::validate_texture(&target)?;
            info.push(Candidate {
                index: i,
                suggested_slot: apex_core::suggest_slot(&name),
                name: name.clone(),
            });
            files.push((target, name));
        }
        let state = app.state::<LiveryDrafts>();
        let mut drafts = state
            .0
            .lock()
            .map_err(|_| Error::Storage("Livery selection unavailable.".into()))?;
        if let Some(token) = token {
            let draft = drafts.get_mut(&token).ok_or(Error::NotFound)?;
            if draft.project != project_id || draft.files.len() + files.len() > 100 {
                return Err(Error::InvalidContent(
                    "Choose up to 100 PNG images in this project.".into(),
                ));
            }
            let start = draft.files.len();
            let mut appended = Vec::new();
            for (i, (path, name)) in files.into_iter().enumerate() {
                let destination = draft._dir.path().join(format!("{}.png", start + i));
                std::fs::copy(path, &destination).map_err(|e| Error::Storage(e.to_string()))?;
                appended.push((destination, name));
            }
            draft.files.extend(appended);
            let files = draft
                .files
                .iter()
                .enumerate()
                .map(|(index, (_, name))| Candidate {
                    index,
                    name: name.clone(),
                    suggested_slot: apex_core::suggest_slot(name),
                })
                .collect();
            return Ok(Some(DraftInfo { token, files }));
        }
        let token = uuid::Uuid::now_v7().to_string();
        drafts.clear();
        drafts.insert(
            token.clone(),
            Draft {
                project: project_id,
                _dir: dir,
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
        let mut drafts = state
            .0
            .lock()
            .map_err(|_| Error::Storage("Livery selection unavailable.".into()))?;
        let draft = drafts.get(&token).ok_or(Error::NotFound)?;
        if assignments.len() != draft.files.len() {
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
            let (path, name) = &draft.files[i];
            files.push((path.clone(), name.clone(), a.slot));
        }
        let id = ws.import_livery(
            &draft.project,
            parent_id.as_deref(),
            existing_id.as_deref(),
            &name,
            &vehicle_id,
            &files,
            cover_index,
        )?;
        drafts.remove(&token);
        Ok(id)
    })
    .await
}
#[tauri::command]
pub async fn cancel_livery(
    token: String,
    state: tauri::State<'_, LiveryDrafts>,
) -> Result<(), CommandError> {
    state
        .0
        .lock()
        .map_err(|_| CommandError {
            code: "draft_unavailable".into(),
            message: "Livery selection unavailable.".into(),
        })?
        .remove(&token);
    Ok(())
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
