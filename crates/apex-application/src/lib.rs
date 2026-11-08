//! Shared application services for desktop, terminal, and future MCP adapters.
//! Presentation layers should call these services instead of reimplementing
//! validation or storage rules.

use apex_core::{Error, Project, ProjectInput, WorkspaceInfo, WorkspaceRepository};
use apex_storage::Workspace;
use std::path::Path;

#[derive(Clone)]
pub struct ApplicationServices {
    workspace: Workspace,
}

impl ApplicationServices {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, Error> {
        Workspace::open(root).map(Self::new)
    }

    pub fn new(workspace: Workspace) -> Self {
        Self { workspace }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn workspace_info(&self) -> Result<WorkspaceInfo, Error> {
        self.workspace.info()
    }

    pub fn rename_workspace(&self, name: &str) -> Result<WorkspaceInfo, Error> {
        apex_core::rename_workspace(&self.workspace, name)
    }

    pub fn projects(&self) -> Result<Vec<Project>, Error> {
        self.workspace.projects()
    }

    pub fn project(&self, id: &str) -> Result<Project, Error> {
        self.workspace.project(id)
    }

    pub fn save_project(&self, id: Option<&str>, fields: ProjectInput) -> Result<Project, Error> {
        self.workspace.save_project(id, fields)
    }

    pub fn archive_project(&self, id: &str, archived: bool) -> Result<Project, Error> {
        self.workspace.archive_project(id, archived)
    }

    pub fn delete_archived_project(&self, id: &str) -> Result<(), Error> {
        self.workspace.delete_archived_project(id)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn independently_opened_clients_share_workspace_reads_and_writes() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let root = temp.path().join("workspace");
        drop(Workspace::open(&root).expect("initialize dummy workspace"));

        let first = ApplicationServices::open(&root).expect("first client");
        let second = ApplicationServices::open(&root).expect("second client");
        let barrier = Arc::new(Barrier::new(3));
        let first_barrier = barrier.clone();
        let first_writer = std::thread::spawn(move || {
            first_barrier.wait();
            first.save_project(
                None,
                ProjectInput {
                    name: "CLI dummy project".into(),
                    ..Default::default()
                },
            )
        });
        let second_barrier = barrier.clone();
        let second_writer = std::thread::spawn(move || {
            second_barrier.wait();
            second.save_project(
                None,
                ProjectInput {
                    name: "Desktop dummy project".into(),
                    ..Default::default()
                },
            )
        });
        barrier.wait();

        first_writer
            .join()
            .expect("first writer thread")
            .expect("first write");
        second_writer
            .join()
            .expect("second writer thread")
            .expect("second write");

        let verifier = ApplicationServices::open(&root).expect("verify workspace");
        let projects = verifier.projects().expect("list shared projects");
        assert_eq!(projects.len(), 2);
        assert!(projects
            .iter()
            .any(|project| project.fields.name == "CLI dummy project"));
        assert!(projects
            .iter()
            .any(|project| project.fields.name == "Desktop dummy project"));
    }
}
