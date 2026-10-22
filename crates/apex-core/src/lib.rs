//! Shared workspace contracts. UI adapters never own persistence or validation rules.
use serde::Serialize;
mod projects;
pub use projects::*;
mod notes;
pub use notes::*;
mod assets;
pub use assets::*;

/// Stable error codes shared by desktop and future interfaces.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Enter a workspace name between 1 and 80 characters.")]
    InvalidName,
    #[error("This workspace uses a newer database version. Update Apex Studio to open it.")]
    NewerSchema,
    #[error("The workspace could not be accessed: {0}")]
    Storage(String),
    #[error("{0}")]
    InvalidProject(String),
    #[error("The project or file no longer exists. Refresh and try again.")]
    NotFound,
    #[error("Restore this project before changing it or importing files.")]
    Archived,
    #[error("{0}")]
    InvalidContent(String),
    #[error("This note changed elsewhere. Reopen it before saving your changes.")]
    Conflict,
}

impl Error {
    /// A machine-readable code, independent of display copy.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidName => "invalid_workspace_name",
            Self::NewerSchema => "newer_schema",
            Self::Storage(_) => "storage_error",
            Self::InvalidProject(_) => "invalid_project",
            Self::NotFound => "not_found",
            Self::Archived => "project_archived",
            Self::InvalidContent(_) => "invalid_content",
            Self::Conflict => "conflict",
        }
    }
}

/// A workspace summary contains no credentials or user-created records.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub name: String,
    pub root: String,
    pub schema_version: u32,
    pub managed_file_count: u64,
}

/// Trim and validate names before writing them through any adapter.
pub fn workspace_name(value: &str) -> Result<&str, Error> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(Error::InvalidName);
    }
    Ok(name)
}

/// Persistence boundary usable by desktop and future CLI services.
pub trait WorkspaceRepository {
    fn info(&self) -> Result<WorkspaceInfo, Error>;
    fn rename(&self, name: &str) -> Result<WorkspaceInfo, Error>;
}

/// Apply the same validation regardless of the caller's interface.
pub fn rename_workspace(
    repository: &impl WorkspaceRepository,
    name: &str,
) -> Result<WorkspaceInfo, Error> {
    repository.rename(workspace_name(name)?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn names_are_trimmed_and_empty_or_control_names_rejected() {
        assert_eq!(workspace_name("  Apex Designs  ").unwrap(), "Apex Designs");
        for name in ["  ", "Apex\nStudio", &"a".repeat(81)] {
            assert!(workspace_name(name).is_err());
        }
    }
}
