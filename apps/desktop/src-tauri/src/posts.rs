use super::{projects::work, CommandError};
use apex_core::{Error, PublishingDraft, PublishingInput, PublishingPreview};
use apex_storage::{PostSourceStatus, PostSummary, Workspace};
use tauri_plugin_dialog::DialogExt;
#[tauri::command]
pub async fn list_posts(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<PostSummary>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.posts()).await
}
#[tauri::command]
pub async fn create_post(workspace: tauri::State<'_, Workspace>) -> Result<String, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.create_post()).await
}
#[tauri::command]
pub async fn post_draft(
    id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<PublishingDraft, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.post(&id)).await
}
#[tauri::command]
pub async fn save_post(
    id: String,
    revision: u64,
    data: PublishingInput,
    workspace: tauri::State<'_, Workspace>,
) -> Result<PublishingDraft, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.save_post(&id, revision, data)).await
}
#[tauri::command]
pub async fn post_sources(
    id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<PostSourceStatus>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.post_sources(&id)).await
}
#[tauri::command]
pub async fn pick_post_images(
    id: String,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<Vec<PublishingPreview>>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.post(&id)?;
        let Some(files) = app
            .dialog()
            .file()
            .add_filter("PNG previews", &["png"])
            .set_title("Add post previews")
            .blocking_pick_files()
        else {
            return Ok(None);
        };
        let paths = files
            .into_iter()
            .map(|f| f.into_path().map_err(|e| Error::Storage(e.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(ws.add_post_images(&format!("editor:{id}"), &paths)?))
    })
    .await
}
#[tauri::command]
pub async fn pin_post_images(
    id: String,
    hashes: Vec<String>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.post(&id)?;
        if hashes.len() > 10 {
            return Err(Error::InvalidContent("Too many previews.".into()));
        }
        ws.pin_content(&format!("editor:{id}"), &hashes)
    })
    .await
}
#[tauri::command]
pub async fn release_post_images(
    id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.release_content(&format!("editor:{id}"))).await
}
#[tauri::command]
pub async fn export_post(
    id: String,
    revision: u64,
    reviewed: bool,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<bool, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.post(&id)?.data.validate(true)?;
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("Apex publishing bundle", &["apexpublish"])
            .set_file_name("post.apexpublish")
            .blocking_save_file()
        else {
            return Ok(false);
        };
        ws.export_post(
            &id,
            revision,
            reviewed,
            &file
                .into_path()
                .map_err(|e| Error::Storage(e.to_string()))?,
        )?;
        Ok(true)
    })
    .await
}
