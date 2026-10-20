//! Temporary, process-local adapter for Phase 1. It is not durable storage.
use std::{collections::BTreeMap, sync::RwLock};

use apex_core::ProjectRepository;
use apex_domain::{AppError, Project};

#[derive(Default)]
pub struct InMemoryProjectRepository {
    projects: RwLock<BTreeMap<String, Project>>,
}

pub mod sqlite;

impl ProjectRepository for InMemoryProjectRepository {
    fn insert(&self, project: Project) -> Result<(), AppError> {
        let key = project.id.0.to_string();
        let mut projects = self.projects.write().map_err(|_| {
            AppError::configuration(
                "storage.lock_poisoned",
                "Der temporäre Projektspeicher ist nicht verfügbar.",
            )
        })?;
        if projects.contains_key(&key) {
            return Err(AppError {
                code: "project.id_conflict".into(),
                message: "Dieses Projekt existiert bereits.".into(),
                retryable: false,
                details: None,
            });
        }
        projects.insert(key, project);
        Ok(())
    }

    fn list(&self) -> Result<Vec<Project>, AppError> {
        self.projects
            .read()
            .map(|projects| projects.values().cloned().collect())
            .map_err(|_| {
                AppError::configuration(
                    "storage.lock_poisoned",
                    "Der temporäre Projektspeicher ist nicht verfügbar.",
                )
            })
    }

    fn update(&self, project: Project, expected_revision: u64) -> Result<Project, AppError> {
        let key = project.id.0.to_string();
        let mut projects = self.projects.write().map_err(|_| {
            AppError::configuration(
                "storage.lock_poisoned",
                "Der temporäre Projektspeicher ist nicht verfügbar.",
            )
        })?;
        let current = projects.get(&key).ok_or_else(|| {
            AppError::validation("project.not_found", "Das Projekt wurde nicht gefunden.")
        })?;
        if current.revision != expected_revision {
            return Err(AppError {
                code: "project.revision_conflict".into(),
                message: "Das Projekt wurde zwischenzeitlich geändert.".into(),
                retryable: false,
                details: Some(format!("current_revision={}", current.revision)),
            });
        }
        let mut updated = project;
        updated.revision = expected_revision + 1;
        projects.insert(key, updated.clone());
        Ok(updated)
    }

    fn archive(
        &self,
        id: &apex_domain::ProjectId,
        expected_revision: u64,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Project, AppError> {
        let key = id.0.to_string();
        let mut projects = self.projects.write().map_err(|_| {
            AppError::configuration(
                "storage.lock_poisoned",
                "Der temporäre Projektspeicher ist nicht verfügbar.",
            )
        })?;
        let project = projects.get_mut(&key).ok_or_else(|| {
            AppError::validation("project.not_found", "Das Projekt wurde nicht gefunden.")
        })?;
        if project.revision != expected_revision {
            return Err(AppError {
                code: "project.revision_conflict".into(),
                message: "Das Projekt wurde zwischenzeitlich geändert.".into(),
                retryable: false,
                details: Some(format!("current_revision={}", project.revision)),
            });
        }
        project.archived_at = Some(now);
        project.updated_at = now;
        project.revision += 1;
        Ok(project.clone())
    }
}
