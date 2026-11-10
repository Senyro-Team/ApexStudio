use super::{projects::work, CommandError};
use apex_storage::{OverlayTemplate, Workspace};

#[tauri::command]
pub async fn list_overlay_templates(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<OverlayTemplate>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.overlay_templates()).await
}

#[tauri::command]
pub async fn overlay_image_data(
    hash: String,
    width: u32,
    height: u32,
    workspace: tauri::State<'_, Workspace>,
) -> Result<String, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.overlay_image_data(&hash, width, height)).await
}

#[tauri::command]
pub async fn save_overlay_template(
    id: Option<String>,
    name: String,
    width: u32,
    height: u32,
    layers_json: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<OverlayTemplate, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.save_overlay_template(id.as_deref(), &name, width, height, &layers_json)).await
}

#[tauri::command]
pub async fn delete_overlay_template(
    id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.delete_overlay_template(&id)).await
}
