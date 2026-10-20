use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct ProjectId(pub Uuid);

impl ProjectId {
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| AppError::validation("project.id.invalid", "Die Projekt-ID ist ungültig."))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct UtcTimestamp(pub DateTime<Utc>);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct LocalDate(NaiveDate);

impl LocalDate {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(Self)
            .map_err(|_| {
                AppError::validation("date.invalid", "Das Datum muss YYYY-MM-DD entsprechen.")
            })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct RelativePathRef(String);

impl RelativePathRef {
    pub fn parse(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into();
        let valid = !value.is_empty()
            && !value.starts_with('/')
            && !value.contains('\\')
            && !value.contains(':')
            && !value.chars().any(char::is_control)
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");
        if !valid {
            return Err(AppError::validation(
                "path_ref.invalid",
                "Der Dateiverweis muss ein normalisierter relativer Pfad sein.",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

macro_rules! version_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
        #[serde(transparent)]
        pub struct $name(pub u32);
    };
}

version_type!(SchemaVersion);
version_type!(IpcVersion);
version_type!(EventVersion);
version_type!(HashAlgorithmVersion);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct ProjectName(String);

impl ProjectName {
    pub fn parse(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into().trim().to_owned();
        if value.is_empty() {
            return Err(AppError::validation(
                "project.name.empty",
                "Der Projektname darf nicht leer sein.",
            ));
        }
        if value.chars().count() > 120 {
            return Err(AppError::validation(
                "project.name.too_long",
                "Der Projektname darf höchstens 120 Zeichen haben.",
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(AppError::validation(
                "project.name.control_character",
                "Der Projektname enthält ein ungültiges Steuerzeichen.",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    pub name: ProjectName,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub revision: u64,
}

impl Project {
    pub fn with_id(id: ProjectId, name: ProjectName, now: DateTime<Utc>) -> Self {
        Self {
            id,
            name,
            description: None,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
            archived_at: None,
            revision: 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct CommissionId(pub Uuid);

impl CommissionId {
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        Uuid::parse_str(value).map(Self).map_err(|_| {
            AppError::validation("commission.id.invalid", "Die Commission-ID ist ungültig.")
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommissionStatus {
    Draft,
    Active,
    OnHold,
    Completed,
    Delivered,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommissionPriority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Unpaid,
    Partial,
    Paid,
    Refunded,
    Waived,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetIdentifierKind {
    Image,
    Decal,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CommissionReference {
    pub kind: String,
    pub label: String,
    pub uri: String,
    pub visible_to_client: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ClientReferenceDto {
    pub kind: String,
    pub label: String,
    pub uri: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CommissionClientDto {
    pub display_id: String,
    pub name: String,
    pub commission_type: Option<String>,
    pub description: Option<String>,
    pub requirements: Vec<String>,
    pub references: Vec<ClientReferenceDto>,
    pub created_at: String,
    pub due_date: Option<String>,
    pub status: CommissionStatus,
    pub design_type: Option<String>,
    pub vehicle_model: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CommissionIdentifier {
    pub kind: AssetIdentifierKind,
    pub id: ExternalRobloxId,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn parse(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into();
        if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(AppError::validation(
                "currency.invalid",
                "Der Währungscode muss aus drei Großbuchstaben bestehen.",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Commission {
    pub id: CommissionId,
    pub project_id: ProjectId,
    pub display_id: String,
    pub name: String,
    pub client_id: Option<String>,
    pub client_name: Option<String>,
    pub organisation_id: Option<String>,
    pub organisation_name: Option<String>,
    pub commission_type: Option<String>,
    pub description: Option<String>,
    pub requirements: Vec<String>,
    pub reference_materials: Vec<CommissionReference>,
    pub created_at: DateTime<Utc>,
    pub due_date: Option<NaiveDate>,
    pub status: CommissionStatus,
    pub priority: Option<CommissionPriority>,
    pub assigned_designer_id: Option<String>,
    pub assigned_designer_handle: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub revision_count: u32,
    pub internal_notes: Option<String>,
    pub design_type: Option<String>,
    pub vehicle_model: Option<String>,
    pub template_reference: Option<String>,
    pub figma_file_url: Option<String>,
    pub export_files: Vec<RelativePathRef>,
    pub roblox_identifiers: Vec<CommissionIdentifier>,
    pub file_hashes: Vec<String>,
    pub version: Option<String>,
    pub total_price_minor: Option<i64>,
    pub currency: Option<CurrencyCode>,
    pub currency_precision: Option<u8>,
    pub payment_status: Option<PaymentStatus>,
    pub payment_method: Option<String>,
    pub usage_rights: Option<String>,
    pub exclusivity: Option<bool>,
    pub license_notes: Option<String>,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CreateCommissionInput {
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub client_name: Option<String>,
    #[serde(default)]
    pub organisation_name: Option<String>,
    #[serde(default)]
    pub commission_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub requirements: Vec<String>,
    #[serde(default)]
    pub due_date: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DeleteCommissionInput {
    pub id: String,
    pub expected_revision: u64,
}

impl Commission {
    pub fn client_share(&self) -> CommissionClientDto {
        CommissionClientDto {
            display_id: self.display_id.clone(),
            name: self.name.clone(),
            commission_type: self.commission_type.clone(),
            description: self.description.clone(),
            requirements: self.requirements.clone(),
            references: self
                .reference_materials
                .iter()
                .filter(|reference| reference.visible_to_client)
                .map(|reference| ClientReferenceDto {
                    kind: reference.kind.clone(),
                    label: reference.label.clone(),
                    uri: reference.uri.clone(),
                })
                .collect(),
            created_at: self.created_at.to_rfc3339(),
            due_date: self.due_date.map(|value| value.to_string()),
            status: self.status.clone(),
            design_type: self.design_type.clone(),
            vehicle_model: self.vehicle_model.clone(),
        }
    }

    pub fn draft(input: CreateCommissionInput, now: DateTime<Utc>) -> Result<Self, AppError> {
        let project_id = ProjectId::parse(&input.project_id)?;
        let due_date = input
            .due_date
            .as_deref()
            .map(|value| LocalDate::parse(value).map(|date| date.0))
            .transpose()?;
        let commission = Self {
            id: CommissionId::new_v7(),
            project_id,
            display_id: String::new(),
            name: input.name,
            client_id: None,
            client_name: input.client_name,
            organisation_id: None,
            organisation_name: input.organisation_name,
            commission_type: input.commission_type,
            description: input.description,
            requirements: input.requirements,
            reference_materials: Vec::new(),
            created_at: now,
            due_date,
            status: CommissionStatus::Draft,
            priority: None,
            assigned_designer_id: None,
            assigned_designer_handle: None,
            started_at: None,
            completed_at: None,
            delivered_at: None,
            revision_count: 0,
            internal_notes: None,
            design_type: None,
            vehicle_model: None,
            template_reference: None,
            figma_file_url: None,
            export_files: Vec::new(),
            roblox_identifiers: Vec::new(),
            file_hashes: Vec::new(),
            version: None,
            total_price_minor: None,
            currency: None,
            currency_precision: None,
            payment_status: None,
            payment_method: None,
            usage_rights: None,
            exclusivity: None,
            license_notes: None,
            revision: 0,
        };
        commission.validate()?;
        Ok(commission)
    }
}

impl Commission {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.name.trim().is_empty() || self.name.chars().count() > 160 {
            return Err(AppError::validation(
                "commission.name.invalid",
                "Der Commission-Name muss 1 bis 160 Zeichen enthalten.",
            ));
        }
        if self.revision_count > i32::MAX as u32 {
            return Err(AppError::validation(
                "commission.revision_count.invalid",
                "Die Revisionszahl ist zu groß.",
            ));
        }
        if self.total_price_minor.is_some_and(|amount| amount < 0) {
            return Err(AppError::validation(
                "commission.price.negative",
                "Der Preis darf nicht negativ sein.",
            ));
        }
        if self.total_price_minor.is_some() != self.currency.is_some()
            || self.currency.is_some() != self.currency_precision.is_some()
        {
            return Err(AppError::validation(
                "commission.price.currency_mismatch",
                "Preis, Währung und Währungspräzision müssen gemeinsam gesetzt sein.",
            ));
        }
        if self
            .currency_precision
            .is_some_and(|precision| precision > 4)
        {
            return Err(AppError::validation(
                "commission.currency_precision.invalid",
                "Die Währungspräzision muss zwischen 0 und 4 liegen.",
            ));
        }
        if self.assigned_designer_id.is_some() != self.assigned_designer_handle.is_some() {
            return Err(AppError::validation(
                "commission.designer.incomplete",
                "Designer-ID und Designer-Handle müssen gemeinsam gesetzt sein.",
            ));
        }
        if self.started_at.is_some_and(|value| value < self.created_at)
            || self
                .completed_at
                .is_some_and(|value| value < self.created_at)
            || self
                .delivered_at
                .is_some_and(|value| value < self.created_at)
            || self
                .started_at
                .zip(self.completed_at)
                .is_some_and(|(start, end)| end < start)
            || self
                .completed_at
                .zip(self.delivered_at)
                .is_some_and(|(complete, delivered)| delivered < complete)
        {
            return Err(AppError::validation(
                "commission.workflow.date_order",
                "Die Workflow-Zeitpunkte liegen in einer ungültigen Reihenfolge.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExternalRobloxId(String);

impl ExternalRobloxId {
    pub fn parse(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into();
        if value.is_empty()
            || !value.bytes().all(|byte| byte.is_ascii_digit())
            || value.bytes().all(|byte| byte == b'0')
        {
            return Err(AppError::validation(
                "roblox_id.invalid",
                "Die Roblox-ID muss eine positive Dezimalzahl sein.",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RobloxIdentifierKind {
    Image,
    Decal,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierResolutionStatus {
    Unknown,
    Pending,
    Resolved,
    Unavailable,
    Failed,
    ManuallyEntered,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierSource {
    UploadResponse,
    OfficialResolution,
    Manual,
    Import,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RobloxAssetIdentifier {
    pub kind: RobloxIdentifierKind,
    pub value: Option<ExternalRobloxId>,
    pub resolution_status: IdentifierResolutionStatus,
    pub source: Option<IdentifierSource>,
    pub verified_at: Option<DateTime<Utc>>,
    pub creator_id: Option<String>,
    pub note: Option<String>,
}

impl RobloxAssetIdentifier {
    pub fn validate(&self) -> Result<(), AppError> {
        let status_has_value = matches!(
            self.resolution_status,
            IdentifierResolutionStatus::Resolved | IdentifierResolutionStatus::ManuallyEntered
        );
        if self.value.is_some() != status_has_value {
            return Err(AppError::validation(
                "roblox_identifier.status_value_mismatch",
                "ID-Wert und Auflösungsstatus passen nicht zusammen.",
            ));
        }
        if matches!(self.resolution_status, IdentifierResolutionStatus::Resolved)
            && self.source.is_none()
        {
            return Err(AppError::validation(
                "roblox_identifier.source_missing",
                "Eine verifizierte ID benötigt eine Herkunft.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CopyIdFormat {
    Numeric,
    RbxassetidUri,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct IdentifierPreferences {
    pub preferred_kind: RobloxIdentifierKind,
    pub copy_format: CopyIdFormat,
}

impl Default for IdentifierPreferences {
    fn default() -> Self {
        Self {
            preferred_kind: RobloxIdentifierKind::Image,
            copy_format: CopyIdFormat::Numeric,
        }
    }
}

pub fn effective_identifier_kind(
    action: Option<RobloxIdentifierKind>,
    project: Option<RobloxIdentifierKind>,
    global: IdentifierPreferences,
) -> RobloxIdentifierKind {
    action.or(project).unwrap_or(global.preferred_kind)
}

pub fn format_identifier(value: &ExternalRobloxId, format: CopyIdFormat) -> String {
    match format {
        CopyIdFormat::Numeric => value.as_str().to_owned(),
        CopyIdFormat::RbxassetidUri => format!("rbxassetid://{}", value.as_str()),
    }
}

pub fn select_preferred_identifier(
    kind: RobloxIdentifierKind,
    identifiers: &[RobloxAssetIdentifier],
) -> Result<&ExternalRobloxId, AppError> {
    let identifier = identifiers
        .iter()
        .find(|item| item.kind == kind)
        .ok_or_else(|| {
            AppError::validation(
                "roblox_identifier.kind_missing",
                "Die gewünschte ID-Art ist nicht vorhanden.",
            )
        })?;
    identifier.validate()?;
    identifier.value.as_ref().ok_or_else(|| {
        AppError::validation(
            "roblox_identifier.unavailable",
            "Für die gewünschte ID-Art liegt noch kein ID-Wert vor.",
        )
    })
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct VersionSet {
    pub schema_version: u32,
    pub ipc_version: u32,
    pub event_version: u32,
    pub hash_algorithm_version: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CommandEnvelope<T> {
    pub version: u32,
    pub request_id: Uuid,
    pub payload: T,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct EventEnvelope<T> {
    pub version: u32,
    pub event_id: Uuid,
    pub kind: String,
    pub entity_id: Uuid,
    pub revision: u64,
    pub occurred_at: DateTime<Utc>,
    pub payload: T,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CreateProjectInput {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct UpdateProjectInput {
    pub id: String,
    pub expected_revision: u64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArchiveProjectInput {
    pub id: String,
    pub expected_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ListProjectsInput {}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProjectDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub revision: u64,
}

impl From<Project> for ProjectDto {
    fn from(project: Project) -> Self {
        Self {
            id: project.id.0.to_string(),
            name: project.name.0,
            description: project.description,
            tags: project.tags,
            created_at: project.created_at.to_rfc3339(),
            updated_at: project.updated_at.to_rfc3339(),
            archived_at: project.archived_at.map(|value| value.to_rfc3339()),
            revision: project.revision,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ErrorDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Option<String>,
}

#[derive(Clone, Debug, Error, Deserialize, Serialize, PartialEq, Eq)]
#[error("{message}")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Option<String>,
}

impl AppError {
    pub fn validation(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
            details: None,
        }
    }

    pub fn configuration(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
            details: None,
        }
    }
}

impl From<&AppError> for ErrorDto {
    fn from(error: &AppError) -> Self {
        Self {
            code: error.code.clone(),
            message: error.message.clone(),
            retryable: error.retryable,
            details: error.details.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_name_is_trimmed_and_validated() {
        assert_eq!(ProjectName::parse("  Apex  ").unwrap().as_str(), "Apex");
        assert_eq!(
            ProjectName::parse("  ").unwrap_err().code,
            "project.name.empty"
        );
        assert_eq!(
            ProjectName::parse("a".repeat(121)).unwrap_err().code,
            "project.name.too_long"
        );
        assert_eq!(
            ProjectName::parse("A\nB").unwrap_err().code,
            "project.name.control_character"
        );
    }

    #[test]
    fn external_roblox_id_remains_an_exact_decimal_string() {
        let value = "9007199254740993123456789";
        let id = ExternalRobloxId::parse(value).unwrap();
        assert_eq!(id.as_str(), value);
        assert!(ExternalRobloxId::parse("0").is_err());
        assert!(ExternalRobloxId::parse("12x").is_err());
    }

    #[test]
    fn identifier_preferences_never_fall_back_to_the_other_asset_kind() {
        let image = RobloxAssetIdentifier {
            kind: RobloxIdentifierKind::Image,
            value: Some(ExternalRobloxId::parse("9007199254740993").unwrap()),
            resolution_status: IdentifierResolutionStatus::Resolved,
            source: Some(IdentifierSource::UploadResponse),
            verified_at: None,
            creator_id: Some("42".into()),
            note: None,
        };
        let decal = RobloxAssetIdentifier {
            kind: RobloxIdentifierKind::Decal,
            value: None,
            resolution_status: IdentifierResolutionStatus::Unavailable,
            source: None,
            verified_at: None,
            creator_id: Some("42".into()),
            note: Some("No documented relation".into()),
        };
        let both = [image, decal];
        let prefs = IdentifierPreferences::default();
        assert_eq!(
            effective_identifier_kind(None, None, prefs),
            RobloxIdentifierKind::Image
        );
        assert_eq!(
            effective_identifier_kind(None, Some(RobloxIdentifierKind::Decal), prefs),
            RobloxIdentifierKind::Decal
        );
        assert_eq!(
            effective_identifier_kind(
                Some(RobloxIdentifierKind::Image),
                Some(RobloxIdentifierKind::Decal),
                prefs
            ),
            RobloxIdentifierKind::Image
        );
        assert_eq!(
            select_preferred_identifier(RobloxIdentifierKind::Decal, &both)
                .unwrap_err()
                .code,
            "roblox_identifier.unavailable"
        );
        let selected = select_preferred_identifier(RobloxIdentifierKind::Image, &both).unwrap();
        assert_eq!(
            format_identifier(selected, CopyIdFormat::Numeric),
            "9007199254740993"
        );
        assert_eq!(
            format_identifier(selected, CopyIdFormat::RbxassetidUri),
            "rbxassetid://9007199254740993"
        );
    }

    #[test]
    fn identifier_status_cannot_claim_a_value_is_resolved_when_empty() {
        let invalid = RobloxAssetIdentifier {
            kind: RobloxIdentifierKind::Image,
            value: None,
            resolution_status: IdentifierResolutionStatus::Resolved,
            source: Some(IdentifierSource::UploadResponse),
            verified_at: None,
            creator_id: None,
            note: None,
        };
        assert_eq!(
            invalid.validate().unwrap_err().code,
            "roblox_identifier.status_value_mismatch"
        );
    }

    #[test]
    fn local_date_and_relative_path_have_canonical_forms() {
        assert!(LocalDate::parse("2026-10-19").is_ok());
        assert!(LocalDate::parse("19.10.2026").is_err());
        assert_eq!(
            RelativePathRef::parse("assets/car.png").unwrap().as_str(),
            "assets/car.png"
        );
        for invalid in [
            "",
            "/root/file",
            "C:/file",
            "assets/../secret",
            "assets\\file",
        ] {
            assert!(
                RelativePathRef::parse(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn project_dto_encodes_uuid_and_utc_timestamp_as_strings() {
        let project = Project::with_id(
            ProjectId(Uuid::now_v7()),
            ProjectName::parse("Livery").unwrap(),
            DateTime::parse_from_rfc3339("2026-10-19T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        let value = serde_json::to_value(ProjectDto::from(project)).unwrap();
        assert!(value["id"].as_str().is_some());
        assert_eq!(value["created_at"], "2026-10-19T10:00:00+00:00");
        assert_eq!(value["revision"], 1);
    }
}
