use super::{projects::work, CommandError};
use apex_core::{Error, Note};
use apex_storage::Workspace;
use std::{collections::HashMap, sync::Mutex};
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
pub struct PendingArchives(pub Mutex<HashMap<String, tempfile::TempPath>>);
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    token: String,
    preview: apex_storage::PackagePreview,
}
#[tauri::command]
pub async fn list_notes(
    project_id: Option<String>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<Note>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.notes(project_id.as_deref())).await
}
#[tauri::command]
pub async fn create_note(
    project_id: Option<String>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Note, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.create_note(project_id.as_deref())).await
}
#[tauri::command]
pub async fn save_note(
    id: String,
    revision: u32,
    title: String,
    document: serde_json::Value,
    markdown: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Note, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.save_note(&id, revision, &title, &document, &markdown)).await
}
#[tauri::command]
pub async fn archive_note(
    id: String,
    revision: u32,
    archived: bool,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Note, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.archive_note(&id, revision, archived)).await
}
#[tauri::command]
pub async fn attach_note_image(
    id: String,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<String>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        let Some(path) = app
            .dialog()
            .file()
            .add_filter("PNG image", &["png"])
            .set_title("Insert image")
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        ws.attach_note_image(
            &id,
            &path
                .into_path()
                .map_err(|e| Error::Storage(e.to_string()))?,
        )
        .map(Some)
    })
    .await
}
#[tauri::command]
pub async fn note_image(
    id: String,
    hash: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<String, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.note_image(&id, &hash)).await
}
#[tauri::command]
pub async fn export_project(
    id: String,
    embedded: bool,
    include_notes: bool,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<bool, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Export project")
            .add_filter("Apex project", &["apexstd"])
            .set_file_name("project.apexstd")
            .blocking_save_file()
        else {
            return Ok(false);
        };
        ws.export_project(
            &id,
            embedded,
            include_notes,
            &path
                .into_path()
                .map_err(|e| Error::Storage(e.to_string()))?,
        )?;
        Ok(true)
    })
    .await
}
#[tauri::command]
pub async fn create_backup(
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<bool, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_title("Back up workspace")
            .add_filter("Apex backup", &["apexbackup"])
            .set_file_name("workspace.apexbackup")
            .blocking_save_file()
        else {
            return Ok(false);
        };
        ws.backup(
            &path
                .into_path()
                .map_err(|e| Error::Storage(e.to_string()))?,
        )?;
        Ok(true)
    })
    .await
}
#[tauri::command]
pub async fn inspect_archive(
    restore: bool,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<Preview>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        let filter = if restore { "apexbackup" } else { "apexstd" };
        let Some(path) = app
            .dialog()
            .file()
            .set_title(if restore {
                "Select workspace backup"
            } else {
                "Import project"
            })
            .add_filter("Apex archive", &[filter])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = path
            .into_path()
            .map_err(|e| Error::Storage(e.to_string()))?;
        if std::fs::metadata(&path)
            .map_err(|e| Error::Storage(e.to_string()))?
            .len()
            > 2 * 1024 * 1024 * 1024
        {
            return Err(Error::InvalidContent(
                "Choose an archive smaller than 2 GB.".into(),
            ));
        }
        let mut file = tempfile::NamedTempFile::new().map_err(|e| Error::Storage(e.to_string()))?;
        std::io::copy(
            &mut std::fs::File::open(path).map_err(|e| Error::Storage(e.to_string()))?,
            &mut file,
        )
        .map_err(|e| Error::Storage(e.to_string()))?;
        let preview = ws.preview_package(file.path())?;
        if preview.kind != if restore { "workspace" } else { "project" } {
            return Err(Error::InvalidContent(
                "Choose the matching project package or workspace backup.".into(),
            ));
        }
        let token = uuid::Uuid::now_v7().to_string();
        use tauri::Manager;
        let state = app.state::<PendingArchives>();
        let mut pending = state
            .0
            .lock()
            .map_err(|_| Error::Storage("Archive selection is unavailable.".into()))?;
        pending.clear();
        pending.insert(token.clone(), file.into_temp_path());
        Ok(Some(Preview { token, preview }))
    })
    .await
}
#[tauri::command]
pub async fn apply_archive(
    token: String,
    restore: bool,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<String, CommandError> {
    let ws = workspace.inner().clone();
    use tauri::Manager;
    let queue = app
        .state::<std::sync::Arc<super::upload_queue::UploadRuntime>>()
        .inner()
        .clone();
    let _change = if restore {
        Some(queue.stop_for_credentials(&ws).await)
    } else {
        None
    };
    work(move || {
        use tauri::Manager;
        let state = app.state::<PendingArchives>();
        let path = state
            .0
            .lock()
            .map_err(|_| Error::Storage("Archive selection is unavailable.".into()))?
            .remove(&token)
            .ok_or(Error::NotFound)?;
        if restore {
            ws.restore_backup(&path).map(|p| {
                format!(
                    "Workspace restored. Previous workspace backup: {}",
                    p.display()
                )
            })
        } else {
            ws.import_package(&path)
        }
    })
    .await
}
