use super::{projects::work, CommandError};
use apex_core::{Error, PublishingDraft, PublishingImage, PublishingInput, PublishingPreview};
use apex_storage::Workspace;
use tauri_plugin_dialog::DialogExt;
#[tauri::command]
pub async fn publishing_draft(
    project_id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<PublishingDraft, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.publishing(&project_id)).await
}
#[tauri::command]
pub async fn publishing_previews(
    images: Vec<PublishingImage>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<PublishingPreview>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        if images.len() > 10 || images.iter().any(|m| !apex_core::valid_hash(&m.hash)) {
            return Err(Error::InvalidContent("Invalid preview selection.".into()));
        }
        ws.publishing_previews(&images)
    })
    .await
}
#[tauri::command]
pub async fn save_publishing(
    project_id: String,
    revision: u64,
    data: PublishingInput,
    workspace: tauri::State<'_, Workspace>,
) -> Result<PublishingDraft, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.save_publishing(&project_id, revision, data)).await
}
#[tauri::command]
pub async fn pick_publishing_images(
    project_id: String,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<Vec<PublishingPreview>>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.editable_project(&project_id)?;
        let Some(files) = app
            .dialog()
            .file()
            .add_filter("PNG previews", &["png"])
            .set_title("Add publishing previews")
            .blocking_pick_files()
        else {
            return Ok(None);
        };
        let paths = files
            .into_iter()
            .map(|f| f.into_path().map_err(|e| Error::Storage(e.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(ws.add_publishing_images(&project_id, &paths)?))
    })
    .await
}
#[tauri::command]
pub async fn export_publishing(
    project_id: String,
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<bool, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.publishing(&project_id)?.data.validate(true)?;
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("Apex publishing bundle", &["apexpublish"])
            .set_file_name("post.apexpublish")
            .blocking_save_file()
        else {
            return Ok(false);
        };
        ws.export_publishing(
            &project_id,
            &file
                .into_path()
                .map_err(|e| Error::Storage(e.to_string()))?,
        )?;
        Ok(true)
    })
    .await
}
