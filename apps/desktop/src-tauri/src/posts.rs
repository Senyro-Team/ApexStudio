use super::{projects::work, CommandError};
use apex_core::{Error, PublishingDraft, PublishingInput, PublishingPreview};
use apex_storage::{PostSourceStatus, PostSummary, Workspace};
use base64::Engine;
use serde::Deserialize;
use tauri_plugin_dialog::DialogExt;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayPreviewInput {
    name: String,
    data_url: String,
}

#[tauri::command]
pub async fn import_overlay_previews(
    post_id: Option<String>,
    project_id: Option<String>,
    images: Vec<OverlayPreviewInput>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<PublishingPreview>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        if post_id.is_some() == project_id.is_some() {
            return Err(Error::InvalidContent(
                "Choose either a post draft or a project.".into(),
            ));
        }
        if let Some(id) = &post_id {
            ws.post(id)?;
        }
        if let Some(id) = &project_id {
            ws.project(id)?;
        }
        if images.is_empty() || images.len() > 10 {
            return Err(Error::InvalidContent(
                "Export between one and ten overlay previews at a time.".into(),
            ));
        }
        let mut total = 0usize;
        let mut decoded = Vec::with_capacity(images.len());
        for image in images {
            let Some(encoded) = image.data_url.strip_prefix("data:image/png;base64,") else {
                return Err(Error::InvalidContent(
                    "Overlay exports must be PNG images.".into(),
                ));
            };
            total = total.saturating_add(encoded.len());
            if total > 80 * 1024 * 1024 {
                return Err(Error::InvalidContent(
                    "The combined overlay export exceeds 60 MB.".into(),
                ));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| Error::InvalidContent("An overlay export is invalid.".into()))?;
            decoded.push((image.name, bytes));
        }
        let owner = post_id.as_deref().map(|id| format!("editor:{id}"));
        ws.add_generated_previews(owner.as_deref(), &decoded)
    })
    .await
}
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
