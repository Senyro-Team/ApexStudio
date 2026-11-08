use super::{projects::work, CommandError};
use apex_storage::{
    RobloxGroupRevenueData, RobloxGroupRevenueImportResult, RobloxGroupSale, RobloxRevenueGroup,
    Workspace,
};

#[tauri::command]
pub async fn roblox_group_revenue(
    workspace: tauri::State<'_, Workspace>,
) -> Result<RobloxGroupRevenueData, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.roblox_group_revenue()).await
}

#[tauri::command]
pub async fn save_roblox_revenue_group(
    group: RobloxRevenueGroup,
    workspace: tauri::State<'_, Workspace>,
) -> Result<RobloxGroupRevenueData, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.save_roblox_revenue_group(&group)?;
        ws.roblox_group_revenue()
    })
    .await
}

#[tauri::command]
pub async fn import_roblox_group_sales(
    group: RobloxRevenueGroup,
    batch_id: String,
    sales: Vec<RobloxGroupSale>,
    workspace: tauri::State<'_, Workspace>,
) -> Result<RobloxGroupRevenueImportResult, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.import_roblox_group_sales(&group, &batch_id, &sales)).await
}

#[tauri::command]
pub async fn remove_roblox_group_revenue(
    group_id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<RobloxGroupRevenueData, CommandError> {
    let ws = workspace.inner().clone();
    work(move || {
        ws.remove_roblox_group_revenue(&group_id)?;
        ws.roblox_group_revenue()
    })
    .await
}
