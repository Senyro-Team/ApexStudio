use super::{projects::work, CommandError};
use apex_core::Error;
use apex_storage::{MediaLibraryItem, Workspace};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn media_library(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<MediaLibraryItem>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.media_library()).await
}

#[tauri::command]
pub async fn pick_media_library_image(
    app: tauri::AppHandle,
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<MediaLibraryItem>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("PNG image", &["png"])
            .set_title("Add image to library")
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|error| Error::Storage(error.to_string()))?;
        Ok(Some(ws.add_media_library_file(&path)?))
    })
    .await
}

#[tauri::command]
pub async fn add_media_library_asset(
    project_id: String,
    asset_id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<MediaLibraryItem, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.add_media_library_asset(&project_id, &asset_id)).await
}

#[tauri::command]
pub async fn add_media_library_url(
    name: String,
    alt_text: String,
    image_url: String,
    default_link_url: Option<String>,
    folder: String,
    tags: Vec<String>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<MediaLibraryItem, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.add_media_library_url(
            &name,
            &alt_text,
            &image_url,
            default_link_url.as_deref(),
            &folder,
            &tags,
        )
    })
    .await
}

#[tauri::command]
pub async fn update_media_library_item(
    id: String,
    name: String,
    alt_text: String,
    image_url: String,
    default_link_url: Option<String>,
    folder: String,
    tags: Vec<String>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<MediaLibraryItem, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.update_media_library_item(
            &id,
            apex_storage::MediaLibraryMetadata {
                name: &name,
                alt_text: &alt_text,
                image_url: &image_url,
                default_link_url: default_link_url.as_deref(),
                folder: &folder,
                tags: &tags,
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn delete_media_library_item(
    id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.delete_media_library_item(&id)).await
}
