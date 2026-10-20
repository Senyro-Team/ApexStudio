#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use apex_core::{
    ArchiveProject, CreateCommission, CreateProject, DeleteCommission, ListCommissions,
    ListProjects, SystemClock, UpdateCommission, UpdateProject, UuidV7Generator,
};
use apex_domain::{
    AppError, ArchiveProjectInput, Commission, CreateCommissionInput, CreateProjectInput,
    DeleteCommissionInput, ErrorDto, ProjectDto, UpdateProjectInput,
};
use apex_images::{DEFAULT_INDEX_LIMIT, ParseSuggestion, parse_filename};
use apex_storage::sqlite::{SqliteCommissionRepository, SqliteDatabase, SqliteProjectRepository};
use tauri::{Manager, State};

struct AppState {
    repository: Arc<SqliteProjectRepository>,
    commissions: Arc<SqliteCommissionRepository>,
}

#[tauri::command]
fn create_project(
    input: CreateProjectInput,
    state: State<'_, AppState>,
) -> Result<ProjectDto, ErrorDto> {
    CreateProject::new(
        state.repository.clone(),
        Arc::new(SystemClock),
        Arc::new(UuidV7Generator),
    )
    .execute_input(input)
    .map(ProjectDto::from)
    .map_err(|error| ErrorDto::from(&error))
}

#[tauri::command]
fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectDto>, ErrorDto> {
    ListProjects::new(state.repository.clone())
        .execute()
        .map(|projects| projects.into_iter().map(ProjectDto::from).collect())
        .map_err(|error: AppError| ErrorDto::from(&error))
}

#[tauri::command]
fn update_project(
    input: UpdateProjectInput,
    state: State<'_, AppState>,
) -> Result<ProjectDto, ErrorDto> {
    UpdateProject::new(state.repository.clone())
        .execute(input)
        .map(ProjectDto::from)
        .map_err(|error| ErrorDto::from(&error))
}

#[tauri::command]
fn archive_project(
    input: ArchiveProjectInput,
    state: State<'_, AppState>,
) -> Result<ProjectDto, ErrorDto> {
    ArchiveProject::new(state.repository.clone(), Arc::new(SystemClock))
        .execute(input)
        .map(ProjectDto::from)
        .map_err(|error| ErrorDto::from(&error))
}

#[tauri::command]
fn create_commission(
    input: CreateCommissionInput,
    state: State<'_, AppState>,
) -> Result<Commission, ErrorDto> {
    CreateCommission::new(state.commissions.clone(), Arc::new(SystemClock))
        .execute(input)
        .map_err(|error| ErrorDto::from(&error))
}

#[tauri::command]
fn list_commissions(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Commission>, ErrorDto> {
    ListCommissions::new(state.commissions.clone())
        .execute(&project_id)
        .map_err(|error| ErrorDto::from(&error))
}

#[tauri::command]
fn update_commission(
    commission: Commission,
    expected_revision: u64,
    state: State<'_, AppState>,
) -> Result<Commission, ErrorDto> {
    UpdateCommission::new(state.commissions.clone())
        .execute(commission, expected_revision)
        .map_err(|error| ErrorDto::from(&error))
}

#[tauri::command]
fn delete_commission(
    input: DeleteCommissionInput,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    DeleteCommission::new(state.commissions.clone())
        .execute(&input.id, input.expected_revision)
        .map_err(|error| ErrorDto::from(&error))
}

#[tauri::command]
fn suggest_image_slot(filename: String) -> ParseSuggestion {
    parse_filename(&filename, DEFAULT_INDEX_LIMIT)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let database = SqliteDatabase::open_default()
                .map_err(|error| std::io::Error::other(error.message))?;
            app.manage(AppState {
                repository: Arc::new(SqliteProjectRepository::new(database.clone())),
                commissions: Arc::new(SqliteCommissionRepository::new(database)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_project,
            list_projects,
            update_project,
            archive_project,
            create_commission,
            list_commissions,
            update_commission,
            delete_commission,
            suggest_image_slot
        ])
        .run(tauri::generate_context!())
        .expect("Apex Studio konnte nicht gestartet werden");
}
