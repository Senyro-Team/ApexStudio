use std::sync::Arc;

use apex_domain::{
    AppError, ArchiveProjectInput, Commission, CommissionId, CreateCommissionInput,
    CreateProjectInput, Project, ProjectId, ProjectName, UpdateProjectInput,
};
use chrono::{DateTime, Utc};

pub trait ProjectRepository: Send + Sync {
    fn insert(&self, project: Project) -> Result<(), AppError>;
    fn list(&self) -> Result<Vec<Project>, AppError>;

    fn get(&self, id: &ProjectId) -> Result<Option<Project>, AppError> {
        Ok(self.list()?.into_iter().find(|project| &project.id == id))
    }

    fn update(&self, _project: Project, _expected_revision: u64) -> Result<Project, AppError> {
        Err(AppError::configuration(
            "repository.update_unavailable",
            "Projektänderungen sind für diesen Speicher nicht verfügbar.",
        ))
    }

    fn archive(
        &self,
        _id: &ProjectId,
        _expected_revision: u64,
        _now: DateTime<Utc>,
    ) -> Result<Project, AppError> {
        Err(AppError::configuration(
            "repository.archive_unavailable",
            "Archivieren ist für diesen Speicher nicht verfügbar.",
        ))
    }

    fn search(
        &self,
        query: Option<&str>,
        include_archived: bool,
    ) -> Result<Vec<Project>, AppError> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|project| {
                (include_archived || project.archived_at.is_none())
                    && query.is_none_or(|needle| {
                        project
                            .name
                            .as_str()
                            .to_lowercase()
                            .contains(&needle.to_lowercase())
                    })
            })
            .collect())
    }
}

pub trait CommissionRepository: Send + Sync {
    fn create(&self, commission: Commission) -> Result<Commission, AppError>;
    fn get(&self, id: &CommissionId) -> Result<Option<Commission>, AppError>;
    fn list_for_project(&self, project_id: &ProjectId) -> Result<Vec<Commission>, AppError>;
    fn update(
        &self,
        commission: Commission,
        expected_revision: u64,
    ) -> Result<Commission, AppError>;
    fn delete(&self, id: &CommissionId, expected_revision: u64) -> Result<(), AppError>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub trait IdGenerator: Send + Sync {
    fn next_project_id(&self) -> ProjectId;
}

pub struct UuidV7Generator;
impl IdGenerator for UuidV7Generator {
    fn next_project_id(&self) -> ProjectId {
        ProjectId::new_v7()
    }
}

pub trait UnitOfWork: Send + Sync {
    fn project_repository(&self) -> &dyn ProjectRepository;
}

pub trait BlobStore: Send + Sync {
    fn resolve_reference(&self, reference: &str) -> Result<String, AppError>;
}

pub trait SecretStore: Send + Sync {
    fn get_secret(&self, key: &str) -> Result<Option<String>, AppError>;
}

pub trait EventSink: Send + Sync {
    fn publish(&self, event_json: &str) -> Result<(), AppError>;
}

pub trait ProviderCapabilities: Send + Sync {
    fn is_available(&self, provider: &str) -> bool;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub struct CreateProject {
    repository: Arc<dyn ProjectRepository>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl CreateProject {
    pub fn new(
        repository: Arc<dyn ProjectRepository>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            repository,
            clock,
            ids,
        }
    }

    pub fn execute(&self, name: String) -> Result<Project, AppError> {
        self.execute_input(CreateProjectInput {
            name,
            description: None,
            tags: Vec::new(),
        })
    }

    pub fn execute_input(&self, input: CreateProjectInput) -> Result<Project, AppError> {
        let name = ProjectName::parse(input.name)?;
        if input
            .description
            .as_ref()
            .is_some_and(|description| description.chars().count() > 10_000)
        {
            return Err(AppError::validation(
                "project.description.too_long",
                "Die Projektbeschreibung darf höchstens 10.000 Zeichen haben.",
            ));
        }
        if input.tags.len() > 100
            || input
                .tags
                .iter()
                .any(|tag| tag.trim().is_empty() || tag.chars().count() > 64)
        {
            return Err(AppError::validation(
                "project.tags.invalid",
                "Ein Projekt darf bis zu 100 Tags mit jeweils 1 bis 64 Zeichen haben.",
            ));
        }
        let mut project = Project::with_id(self.ids.next_project_id(), name, self.clock.now());
        project.description = input.description;
        project.tags = input
            .tags
            .into_iter()
            .map(|tag| tag.trim().to_owned())
            .collect();
        self.repository.insert(project.clone())?;
        Ok(project)
    }
}

pub struct ListProjects {
    repository: Arc<dyn ProjectRepository>,
}

impl ListProjects {
    pub fn new(repository: Arc<dyn ProjectRepository>) -> Self {
        Self { repository }
    }

    pub fn execute(&self) -> Result<Vec<Project>, AppError> {
        self.repository.list()
    }

    pub fn search(
        &self,
        query: Option<&str>,
        include_archived: bool,
    ) -> Result<Vec<Project>, AppError> {
        self.repository.search(query, include_archived)
    }
}

pub struct UpdateProject {
    repository: Arc<dyn ProjectRepository>,
}

impl UpdateProject {
    pub fn new(repository: Arc<dyn ProjectRepository>) -> Self {
        Self { repository }
    }

    pub fn execute(&self, input: UpdateProjectInput) -> Result<Project, AppError> {
        let id = ProjectId::parse(&input.id)?;
        let mut project = self.repository.get(&id)?.ok_or_else(|| {
            AppError::validation("project.not_found", "Das Projekt wurde nicht gefunden.")
        })?;
        project.name = ProjectName::parse(input.name)?;
        if input
            .description
            .as_ref()
            .is_some_and(|value| value.chars().count() > 10_000)
        {
            return Err(AppError::validation(
                "project.description.too_long",
                "Die Projektbeschreibung darf höchstens 10.000 Zeichen haben.",
            ));
        }
        if input.tags.len() > 100
            || input
                .tags
                .iter()
                .any(|tag| tag.trim().is_empty() || tag.chars().count() > 64)
        {
            return Err(AppError::validation(
                "project.tags.invalid",
                "Ein Projekt darf bis zu 100 Tags mit jeweils 1 bis 64 Zeichen haben.",
            ));
        }
        project.description = input.description;
        project.tags = input
            .tags
            .into_iter()
            .map(|tag| tag.trim().to_owned())
            .collect();
        self.repository.update(project, input.expected_revision)
    }
}

pub struct ArchiveProject {
    repository: Arc<dyn ProjectRepository>,
    clock: Arc<dyn Clock>,
}

pub struct CreateCommission {
    repository: Arc<dyn CommissionRepository>,
    clock: Arc<dyn Clock>,
}

impl CreateCommission {
    pub fn new(repository: Arc<dyn CommissionRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repository, clock }
    }
    pub fn execute(&self, input: CreateCommissionInput) -> Result<Commission, AppError> {
        self.repository
            .create(Commission::draft(input, self.clock.now())?)
    }
}

pub struct ListCommissions {
    repository: Arc<dyn CommissionRepository>,
}

impl ListCommissions {
    pub fn new(repository: Arc<dyn CommissionRepository>) -> Self {
        Self { repository }
    }
    pub fn execute(&self, project_id: &str) -> Result<Vec<Commission>, AppError> {
        self.repository
            .list_for_project(&ProjectId::parse(project_id)?)
    }
}

pub struct UpdateCommission {
    repository: Arc<dyn CommissionRepository>,
}

impl UpdateCommission {
    pub fn new(repository: Arc<dyn CommissionRepository>) -> Self {
        Self { repository }
    }
    pub fn execute(
        &self,
        commission: Commission,
        expected_revision: u64,
    ) -> Result<Commission, AppError> {
        self.repository.update(commission, expected_revision)
    }
}

pub struct DeleteCommission {
    repository: Arc<dyn CommissionRepository>,
}

impl DeleteCommission {
    pub fn new(repository: Arc<dyn CommissionRepository>) -> Self {
        Self { repository }
    }
    pub fn execute(&self, id: &str, expected_revision: u64) -> Result<(), AppError> {
        let id = CommissionId::parse(id)?;
        self.repository.delete(&id, expected_revision)
    }
}

impl ArchiveProject {
    pub fn new(repository: Arc<dyn ProjectRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repository, clock }
    }

    pub fn execute(&self, input: ArchiveProjectInput) -> Result<Project, AppError> {
        self.repository.archive(
            &ProjectId::parse(&input.id)?,
            input.expected_revision,
            self.clock.now(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    #[derive(Default)]
    struct TestRepository(Mutex<Vec<Project>>);

    impl ProjectRepository for TestRepository {
        fn insert(&self, project: Project) -> Result<(), AppError> {
            self.0.lock().unwrap().push(project);
            Ok(())
        }

        fn list(&self) -> Result<Vec<Project>, AppError> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    struct FixedClock(DateTime<Utc>);
    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    struct FixedIds;
    impl IdGenerator for FixedIds {
        fn next_project_id(&self) -> ProjectId {
            ProjectId(Uuid::parse_str("0199f16c-7abc-7000-8000-000000000001").unwrap())
        }
    }

    #[test]
    fn create_and_list_share_the_repository_and_normalized_domain_value() {
        let repository = Arc::new(TestRepository::default());
        let now = DateTime::parse_from_rfc3339("2026-10-19T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let create = CreateProject::new(
            repository.clone(),
            Arc::new(FixedClock(now)),
            Arc::new(FixedIds),
        );
        let created = create.execute("  Apex Project  ".into()).unwrap();
        let listed = ListProjects::new(repository).execute().unwrap();
        assert_eq!(listed, vec![created.clone()]);
        assert_eq!(created.name.as_str(), "Apex Project");
        assert_eq!(
            created.id.0.to_string(),
            "0199f16c-7abc-7000-8000-000000000001"
        );
        assert_eq!(created.created_at, now);
    }

    #[test]
    fn invalid_create_returns_the_stable_domain_error() {
        let repository = Arc::new(TestRepository::default());
        let now = Utc::now();
        let create = CreateProject::new(repository, Arc::new(FixedClock(now)), Arc::new(FixedIds));
        let error = create.execute("  ".into()).unwrap_err();
        assert_eq!(error.code, "project.name.empty");
        assert!(!error.retryable);
        assert!(error.details.is_none());
    }
}
