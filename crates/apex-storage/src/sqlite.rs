use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use apex_core::{CommissionRepository, ProjectRepository};
use apex_domain::{
    AppError, Commission, CommissionId, CommissionStatus, CopyIdFormat, IdentifierPreferences,
    Project, ProjectId, ProjectName, RobloxIdentifierKind,
};
use chrono::{DateTime, Datelike, Utc};
use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, params, types::Value,
};
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 2;

pub struct SqliteDatabase {
    path: PathBuf,
    connection: Mutex<Connection>,
}

impl SqliteDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Arc<Self>, AppError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| storage_error("database.directory", error.to_string()))?;
        }
        let connection = Connection::open(&path).map_err(sqlite_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sqlite_error)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(sqlite_error)?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(sqlite_error)?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(sqlite_error)?;
        migrate(&connection)?;
        Ok(Arc::new(Self {
            path,
            connection: Mutex::new(connection),
        }))
    }

    pub fn open_default() -> Result<Arc<Self>, AppError> {
        let root = dirs::data_local_dir().ok_or_else(|| {
            storage_error(
                "database.path_unavailable",
                "Der lokale Datenordner ist nicht verfügbar.",
            )
        })?;
        Self::open(root.join("Apex Studio").join("apex-studio.sqlite3"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, AppError> {
        self.connection.lock().map_err(|_| {
            storage_error(
                "database.lock_poisoned",
                "Die lokale Datenbank ist nicht verfügbar.",
            )
        })
    }

    pub fn backup_to(&self, destination: impl AsRef<Path>) -> Result<(), AppError> {
        let destination = destination.as_ref();
        if destination.exists() {
            return Err(storage_error(
                "backup.destination_exists",
                "Die Sicherungsdatei existiert bereits; sie wurde nicht überschrieben.",
            ));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| storage_error("backup.directory", error.to_string()))?;
        }
        let temp_path = destination.with_extension("backup-partial");
        if temp_path.exists() {
            std::fs::remove_file(&temp_path)
                .map_err(|error| storage_error("backup.stale_partial", error.to_string()))?;
        }
        let source = self.connection()?;
        let mut target = Connection::open(&temp_path).map_err(sqlite_error)?;
        {
            let backup =
                rusqlite::backup::Backup::new(&source, &mut target).map_err(sqlite_error)?;
            backup
                .run_to_completion(32, std::time::Duration::from_millis(50), None)
                .map_err(sqlite_error)?;
        }
        drop(target);
        drop(source);
        std::fs::rename(&temp_path, destination)
            .map_err(|error| storage_error("backup.publish", error.to_string()))?;
        Ok(())
    }

    pub fn enqueue_upload(
        &self,
        creator_profile_id: &str,
        asset_version_id: Option<&str>,
        upload_type: &str,
        file_sha256: &str,
        snapshot_json: &str,
    ) -> Result<UploadJob, AppError> {
        if creator_profile_id.trim().is_empty()
            || !matches!(upload_type, "image" | "decal")
            || file_sha256.len() != 64
            || !file_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(AppError::validation(
                "upload.intent.invalid",
                "Creator, Uploadtyp oder SHA-256 sind ungültig.",
            ));
        }
        let snapshot_json = validate_upload_snapshot(snapshot_json)?;
        let now = Utc::now();
        let job = UploadJob {
            id: Uuid::now_v7().to_string(),
            creator_profile_id: creator_profile_id.into(),
            asset_version_id: asset_version_id.map(str::to_owned),
            upload_type: upload_type.into(),
            file_sha256: file_sha256.to_ascii_lowercase(),
            state: UploadState::Queued,
            operation_id: None,
            snapshot_json,
            revision: 1,
            lease_owner: None,
            lease_expires_at: None,
            created_at: now,
            updated_at: now,
        };
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
        tx.execute("INSERT INTO upload_jobs_v2(id,creator_profile_id,asset_version_id,upload_type,file_sha256,state,snapshot_json,revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,'queued',?6,1,?7,?7)", params![job.id,job.creator_profile_id,job.asset_version_id,job.upload_type,job.file_sha256,job.snapshot_json,now.to_rfc3339()]).map_err(sqlite_error)?;
        insert_upload_event(&tx, &job, "enqueued", now)?;
        tx.commit().map_err(sqlite_error)?;
        Ok(job)
    }

    /// Claims one job with a SQLite CAS lease. Expired in-flight requests become unknown, never re-posted.
    pub fn claim_upload(
        &self,
        worker: &str,
        lease_seconds: i64,
    ) -> Result<Option<UploadJob>, AppError> {
        if worker.is_empty() || worker.len() > 120 || !(10..=3600).contains(&lease_seconds) {
            return Err(AppError::validation(
                "upload.lease.invalid",
                "Worker oder Lease-Dauer sind ungültig.",
            ));
        }
        let now = Utc::now();
        let expires = now + chrono::Duration::seconds(lease_seconds);
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
        let expired: Vec<String> = {
            let mut stmt = tx.prepare("SELECT id FROM upload_jobs_v2 WHERE state='uploading' AND lease_expires_at < ?1").map_err(sqlite_error)?;
            let rows = stmt
                .query_map([now.to_rfc3339()], |row| row.get(0))
                .map_err(sqlite_error)?;
            rows.collect::<Result<Vec<String>, _>>()
                .map_err(sqlite_error)?
        };
        for id in expired {
            tx.execute("UPDATE upload_jobs_v2 SET state='unknown_remote_outcome',lease_owner=NULL,lease_expires_at=NULL,revision=revision+1,updated_at=?2 WHERE id=?1 AND state='uploading'", params![id,now.to_rfc3339()]).map_err(sqlite_error)?;
            let mut job = load_upload_job(&tx, &id)?
                .ok_or_else(|| storage_error("upload.job_missing", "Uploadjob nicht gefunden."))?;
            job.state = UploadState::UnknownRemoteOutcome;
            insert_upload_event(&tx, &job, "lease_expired_unknown_outcome", now)?;
        }
        let candidate: Option<String> = tx.query_row("SELECT id FROM upload_jobs_v2 WHERE state IN ('queued','failed_retryable') AND lease_owner IS NULL ORDER BY created_at,id LIMIT 1", [], |row| row.get(0)).optional().map_err(sqlite_error)?;
        let result = if let Some(id) = candidate {
            let changed = tx.execute("UPDATE upload_jobs_v2 SET state='uploading',lease_owner=?2,lease_expires_at=?3,revision=revision+1,updated_at=?4 WHERE id=?1 AND state IN ('queued','failed_retryable') AND lease_owner IS NULL", params![id,worker,expires.to_rfc3339(),now.to_rfc3339()]).map_err(sqlite_error)?;
            if changed == 1 {
                let job = load_upload_job(&tx, &id)?.ok_or_else(|| {
                    storage_error("upload.job_missing", "Uploadjob nicht gefunden.")
                })?;
                insert_upload_event(&tx, &job, "claimed", now)?;
                Some(job)
            } else {
                None
            }
        } else {
            None
        };
        tx.commit().map_err(sqlite_error)?;
        Ok(result)
    }

    pub fn save_upload_operation(
        &self,
        job_id: &str,
        worker: &str,
        operation_id: &str,
    ) -> Result<UploadJob, AppError> {
        if operation_id.is_empty()
            || operation_id.len() > 160
            || !operation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(AppError::validation(
                "upload.operation.invalid",
                "Die Roblox-Operation-ID ist ungültig.",
            ));
        }
        self.transition_claimed_upload(
            job_id,
            worker,
            UploadState::AwaitingOperation,
            Some(operation_id),
            "remote_operation_saved",
        )
    }

    pub fn mark_upload_unknown(&self, job_id: &str, worker: &str) -> Result<UploadJob, AppError> {
        self.transition_claimed_upload(
            job_id,
            worker,
            UploadState::UnknownRemoteOutcome,
            None,
            "transport_outcome_unknown",
        )
    }

    fn transition_claimed_upload(
        &self,
        job_id: &str,
        worker: &str,
        state: UploadState,
        operation_id: Option<&str>,
        event: &str,
    ) -> Result<UploadJob, AppError> {
        let now = Utc::now();
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
        let changed = tx.execute("UPDATE upload_jobs_v2 SET state=?3,operation_id=COALESCE(?4,operation_id),lease_owner=NULL,lease_expires_at=NULL,revision=revision+1,updated_at=?5 WHERE id=?1 AND lease_owner=?2 AND state='uploading'", params![job_id,worker,state.as_str(),operation_id,now.to_rfc3339()]).map_err(sqlite_error)?;
        if changed == 0 {
            return Err(AppError::validation(
                "upload.lease_conflict",
                "Die Upload-Lease ist abgelaufen oder gehört einem anderen Worker.",
            ));
        }
        let job = load_upload_job(&tx, job_id)?
            .ok_or_else(|| storage_error("upload.job_missing", "Uploadjob nicht gefunden."))?;
        insert_upload_event(&tx, &job, event, now)?;
        tx.commit().map_err(sqlite_error)?;
        Ok(job)
    }

    pub fn upload_history(&self, limit: usize) -> Result<Vec<UploadJob>, AppError> {
        if limit == 0 || limit > 10_000 {
            return Err(AppError::validation(
                "upload.history.limit",
                "Die Historybegrenzung muss zwischen 1 und 10.000 liegen.",
            ));
        }
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT id FROM upload_jobs_v2 ORDER BY created_at DESC,id DESC LIMIT ?1")
            .map_err(sqlite_error)?;
        let ids = statement
            .query_map([limit as i64], |row| row.get::<_, String>(0))
            .map_err(sqlite_error)?;
        ids.map(|id| {
            id.map_err(sqlite_error).and_then(|id| {
                load_upload_job(&connection, &id)?
                    .ok_or_else(|| storage_error("upload.job_missing", "Uploadjob nicht gefunden."))
            })
        })
        .collect()
    }

    pub fn save_identifier_preferences(
        &self,
        project_id: Option<&ProjectId>,
        preferences: IdentifierPreferences,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let kind = match preferences.preferred_kind {
            RobloxIdentifierKind::Image => "image",
            RobloxIdentifierKind::Decal => "decal",
        };
        let format = match preferences.copy_format {
            CopyIdFormat::Numeric => "numeric",
            CopyIdFormat::RbxassetidUri => "rbxassetid_uri",
        };
        let connection = self.connection()?;
        if let Some(project_id) = project_id {
            connection.execute("INSERT INTO asset_identifier_preferences(project_id,preferred_kind,copy_format,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(project_id) DO UPDATE SET preferred_kind=excluded.preferred_kind,copy_format=excluded.copy_format,updated_at=excluded.updated_at", params![project_id.0.to_string(),kind,format,now]).map_err(sqlite_error)?;
        } else {
            connection.execute("UPDATE asset_identifier_settings SET preferred_kind=?1,copy_format=?2,updated_at=?3 WHERE id=1", params![kind,format,now]).map_err(sqlite_error)?;
        }
        Ok(())
    }

    pub fn identifier_preferences(
        &self,
        project_id: Option<&ProjectId>,
    ) -> Result<IdentifierPreferences, AppError> {
        let connection = self.connection()?;
        let row: Option<(String, String)> = if let Some(project_id) = project_id {
            connection.query_row("SELECT preferred_kind,copy_format FROM asset_identifier_preferences WHERE project_id=?1", [project_id.0.to_string()], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(sqlite_error)?
        } else {
            None
        };
        let row = match row {
            Some(value) => Some(value),
            None => connection
                .query_row(
                    "SELECT preferred_kind,copy_format FROM asset_identifier_settings WHERE id=1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sqlite_error)?,
        };
        let (kind, format) = row.ok_or_else(|| {
            storage_error("identifier.preferences_missing", "ID-Präferenzen fehlen.")
        })?;
        let preferred_kind = match kind.as_str() {
            "image" => RobloxIdentifierKind::Image,
            "decal" => RobloxIdentifierKind::Decal,
            _ => {
                return Err(storage_error(
                    "identifier.preferences_invalid",
                    "Die gespeicherte ID-Präferenz ist ungültig.",
                ));
            }
        };
        let copy_format = match format.as_str() {
            "numeric" => CopyIdFormat::Numeric,
            "rbxassetid_uri" => CopyIdFormat::RbxassetidUri,
            _ => {
                return Err(storage_error(
                    "identifier.preferences_invalid",
                    "Das gespeicherte Kopierformat ist ungültig.",
                ));
            }
        };
        Ok(IdentifierPreferences {
            preferred_kind,
            copy_format,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadState {
    Queued,
    Uploading,
    AwaitingOperation,
    AwaitingModeration,
    Succeeded,
    FailedRetryable,
    FailedFinal,
    UnknownRemoteOutcome,
    CancelRequested,
    Cancelled,
}
impl UploadState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Uploading => "uploading",
            Self::AwaitingOperation => "awaiting_operation",
            Self::AwaitingModeration => "awaiting_moderation",
            Self::Succeeded => "succeeded",
            Self::FailedRetryable => "failed_retryable",
            Self::FailedFinal => "failed_final",
            Self::UnknownRemoteOutcome => "unknown_remote_outcome",
            Self::CancelRequested => "cancel_requested",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadJob {
    pub id: String,
    pub creator_profile_id: String,
    pub asset_version_id: Option<String>,
    pub upload_type: String,
    pub file_sha256: String,
    pub state: UploadState,
    pub operation_id: Option<String>,
    pub snapshot_json: String,
    pub revision: i64,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn load_upload_job(connection: &Connection, id: &str) -> Result<Option<UploadJob>, AppError> {
    connection.query_row("SELECT id,creator_profile_id,asset_version_id,upload_type,file_sha256,state,operation_id,snapshot_json,revision,lease_owner,lease_expires_at,created_at,updated_at FROM upload_jobs_v2 WHERE id=?1", [id], |row| {
        let parse = |index| -> rusqlite::Result<DateTime<Utc>> { let text:String=row.get(index)?; DateTime::parse_from_rfc3339(&text).map(|d|d.with_timezone(&Utc)).map_err(|e|rusqlite::Error::FromSqlConversionFailure(index,rusqlite::types::Type::Text,Box::new(e))) };
        let state:String=row.get(5)?;
        let state=match state.as_str(){"queued"=>UploadState::Queued,"uploading"=>UploadState::Uploading,"awaiting_operation"=>UploadState::AwaitingOperation,"awaiting_moderation"=>UploadState::AwaitingModeration,"succeeded"=>UploadState::Succeeded,"failed_retryable"=>UploadState::FailedRetryable,"failed_final"=>UploadState::FailedFinal,"unknown_remote_outcome"=>UploadState::UnknownRemoteOutcome,"cancel_requested"=>UploadState::CancelRequested,"cancelled"=>UploadState::Cancelled,_=>return Err(rusqlite::Error::InvalidQuery)};
        let lease:Option<String>=row.get(10)?;
        let lease_expires_at=lease.map(|s|DateTime::parse_from_rfc3339(&s).map(|d|d.with_timezone(&Utc)).map_err(|e|rusqlite::Error::FromSqlConversionFailure(10,rusqlite::types::Type::Text,Box::new(e)))).transpose()?;
        Ok(UploadJob{id:row.get(0)?,creator_profile_id:row.get(1)?,asset_version_id:row.get(2)?,upload_type:row.get(3)?,file_sha256:row.get(4)?,state,operation_id:row.get(6)?,snapshot_json:row.get(7)?,revision:row.get(8)?,lease_owner:row.get(9)?,lease_expires_at,created_at:parse(11)?,updated_at:parse(12)?})
    }).optional().map_err(sqlite_error)
}

fn insert_upload_event(
    tx: &Transaction<'_>,
    job: &UploadJob,
    event: &str,
    at: DateTime<Utc>,
) -> Result<(), AppError> {
    let payload = serde_json::json!({"state":job.state.as_str(),"creator_profile_id":job.creator_profile_id,"upload_type":job.upload_type,"file_sha256":job.file_sha256,"operation_id":job.operation_id,"revision":job.revision});
    tx.execute("INSERT INTO upload_events(id,job_id,event_type,payload_json,created_at) VALUES(?1,?2,?3,?4,?5)",params![Uuid::now_v7().to_string(),job.id,event,payload.to_string(),at.to_rfc3339()]).map_err(sqlite_error)?;
    Ok(())
}

fn validate_upload_snapshot(snapshot: &str) -> Result<String, AppError> {
    let value: serde_json::Value = serde_json::from_str(snapshot).map_err(|_| {
        AppError::validation(
            "upload.snapshot.invalid",
            "Der Upload-Snapshot ist kein gültiges JSON.",
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        AppError::validation(
            "upload.snapshot.invalid",
            "Der Upload-Snapshot muss ein JSON-Objekt sein.",
        )
    })?;
    const ALLOWED: &[&str] = &[
        "asset_version_id",
        "parser_version",
        "side",
        "slot_index",
        "width",
        "height",
        "byte_count",
        "file_sha256",
        "pixel_sha256_v1",
        "phash_v1",
    ];
    if object.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(AppError::validation(
            "upload.snapshot.field_not_allowed",
            "Der Upload-Snapshot enthält ein nicht freigegebenes Feld.",
        ));
    }
    if object
        .values()
        .any(|value| !(value.is_string() || value.is_number() || value.is_null()))
    {
        return Err(AppError::validation(
            "upload.snapshot.value_invalid",
            "Der Upload-Snapshot enthält einen ungültigen Feldwert.",
        ));
    }
    serde_json::to_string(&value).map_err(json_error)
}

pub struct SqliteProjectRepository {
    database: Arc<SqliteDatabase>,
}

impl SqliteProjectRepository {
    pub fn new(database: Arc<SqliteDatabase>) -> Self {
        Self { database }
    }
    pub fn database(&self) -> &Arc<SqliteDatabase> {
        &self.database
    }
}

pub struct SqliteCommissionRepository {
    database: Arc<SqliteDatabase>,
}

impl SqliteCommissionRepository {
    pub fn new(database: Arc<SqliteDatabase>) -> Self {
        Self { database }
    }
}

impl CommissionRepository for SqliteCommissionRepository {
    fn create(&self, mut commission: Commission) -> Result<Commission, AppError> {
        commission.validate()?;
        let mut connection = self.database.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
        let project_state: Option<Option<String>> = transaction
            .query_row(
                "SELECT archived_at FROM projects WHERE id=?1",
                [&commission.project_id.0.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)?;
        match project_state {
            None => {
                return Err(AppError::validation(
                    "project.not_found",
                    "Das Projekt wurde nicht gefunden.",
                ));
            }
            Some(Some(_)) => {
                return Err(AppError::validation(
                    "project.archived",
                    "Zu einem archivierten Projekt kann keine Commission angelegt werden.",
                ));
            }
            Some(None) => {}
        }
        ensure_party(
            &transaction,
            "clients",
            &mut commission.client_id,
            &commission.client_name,
        )?;
        ensure_party(
            &transaction,
            "organisations",
            &mut commission.organisation_id,
            &commission.organisation_name,
        )?;
        if let (Some(id), Some(handle)) = (
            &commission.assigned_designer_id,
            &commission.assigned_designer_handle,
        ) {
            transaction.execute("INSERT INTO designers(id,handle,created_at) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET handle=excluded.handle", params![id,handle,commission.created_at.to_rfc3339()]).map_err(sqlite_error)?;
        }
        let year = commission.created_at.year();
        let number: i64 = transaction.query_row("INSERT INTO commission_counters(workspace_id,year,next_number) VALUES('local',?1,1) ON CONFLICT(workspace_id,year) DO UPDATE SET next_number=commission_counters.next_number+1 RETURNING next_number", [year], |row| row.get(0)).map_err(sqlite_error)?;
        commission.display_id = format!("COM-{year}-{number:04}");
        commission.revision = 1;
        let payload = serde_json::to_string(&commission).map_err(json_error)?;
        transaction.execute("INSERT INTO commissions(id,project_id,display_id,name,client_id,client_name,organisation_id,organisation_name,commission_type,description,requirements_json,references_json,created_at,due_date,status,priority,assigned_designer_id,assigned_designer_handle,started_at,completed_at,delivered_at,revision_count,internal_notes,design_type,vehicle_model,template_reference,figma_file_url,export_files_json,roblox_identifiers_json,file_hashes_json,version,total_price_minor,currency,currency_precision,payment_status,payment_method,usage_rights,exclusivity,license_notes,payload_json,revision) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36,?37,?38,?39,?40,?41)", rusqlite::params_from_iter(commission_sql_values(&commission, &payload)?)).map_err(sqlite_error)?;
        record_change(
            &transaction,
            "commission",
            &commission.id.0.to_string(),
            commission.revision,
            &commission,
            commission.created_at,
        )?;
        transaction.commit().map_err(sqlite_error)?;
        Ok(commission)
    }

    fn get(&self, id: &CommissionId) -> Result<Option<Commission>, AppError> {
        let connection = self.database.connection()?;
        let raw: Option<String> = connection
            .query_row(
                "SELECT payload_json FROM commissions WHERE id=?1",
                [id.0.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)?;
        raw.map(|value| serde_json::from_str(&value).map_err(json_error))
            .transpose()
    }

    fn list_for_project(&self, project_id: &ProjectId) -> Result<Vec<Commission>, AppError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare("SELECT payload_json FROM commissions WHERE project_id=?1 ORDER BY created_at DESC,display_id DESC").map_err(sqlite_error)?;
        let rows = statement
            .query_map([project_id.0.to_string()], |row| row.get::<_, String>(0))
            .map_err(sqlite_error)?;
        rows.map(|row| {
            row.map_err(sqlite_error)
                .and_then(|json| serde_json::from_str(&json).map_err(json_error))
        })
        .collect()
    }

    fn update(
        &self,
        mut commission: Commission,
        expected_revision: u64,
    ) -> Result<Commission, AppError> {
        commission.validate()?;
        let mut connection = self.database.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
        let current: Option<String> = transaction
            .query_row(
                "SELECT payload_json FROM commissions WHERE id=?1",
                [commission.id.0.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite_error)?;
        let Some(current) = current else {
            return Err(AppError::validation(
                "commission.not_found",
                "Die Commission wurde nicht gefunden.",
            ));
        };
        let current: Commission = serde_json::from_str(&current).map_err(json_error)?;
        if current.revision != expected_revision {
            return Err(AppError {
                code: "commission.revision_conflict".into(),
                message: "Die Commission wurde zwischenzeitlich geändert.".into(),
                retryable: false,
                details: Some(format!(
                    "expected_revision={expected_revision};current_revision={}",
                    current.revision
                )),
            });
        }
        if commission.project_id != current.project_id
            || commission.display_id != current.display_id
        {
            return Err(AppError::validation(
                "commission.identity_immutable",
                "Projektzuordnung und sichtbare Commission-ID können nicht geändert werden.",
            ));
        }
        ensure_party(
            &transaction,
            "clients",
            &mut commission.client_id,
            &commission.client_name,
        )?;
        ensure_party(
            &transaction,
            "organisations",
            &mut commission.organisation_id,
            &commission.organisation_name,
        )?;
        if let (Some(id), Some(handle)) = (
            &commission.assigned_designer_id,
            &commission.assigned_designer_handle,
        ) {
            transaction.execute("INSERT INTO designers(id,handle,created_at) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET handle=excluded.handle", params![id,handle,commission.created_at.to_rfc3339()]).map_err(sqlite_error)?;
        }
        commission.revision = expected_revision.checked_add(1).ok_or_else(|| {
            storage_error(
                "commission.revision.invalid",
                "Die Commission-Version ist zu groß.",
            )
        })?;
        let payload = serde_json::to_string(&commission).map_err(json_error)?;
        let mut values = commission_sql_values(&commission, &payload)?;
        values.push(Value::Integer(i64::try_from(expected_revision).map_err(
            |_| {
                storage_error(
                    "commission.revision.invalid",
                    "Die Commission-Version ist zu groß.",
                )
            },
        )?));
        let changed = transaction.execute("UPDATE commissions SET project_id=?2,display_id=?3,name=?4,client_id=?5,client_name=?6,organisation_id=?7,organisation_name=?8,commission_type=?9,description=?10,requirements_json=?11,references_json=?12,created_at=?13,due_date=?14,status=?15,priority=?16,assigned_designer_id=?17,assigned_designer_handle=?18,started_at=?19,completed_at=?20,delivered_at=?21,revision_count=?22,internal_notes=?23,design_type=?24,vehicle_model=?25,template_reference=?26,figma_file_url=?27,export_files_json=?28,roblox_identifiers_json=?29,file_hashes_json=?30,version=?31,total_price_minor=?32,currency=?33,currency_precision=?34,payment_status=?35,payment_method=?36,usage_rights=?37,exclusivity=?38,license_notes=?39,payload_json=?40,revision=?41 WHERE id=?1 AND revision=?42", rusqlite::params_from_iter(values)).map_err(sqlite_error)?;
        if changed == 0 {
            return Err(AppError::validation(
                "commission.revision_conflict",
                "Die Commission wurde zwischenzeitlich geändert.",
            ));
        }
        record_change(
            &transaction,
            "commission",
            &commission.id.0.to_string(),
            commission.revision,
            &commission,
            Utc::now(),
        )?;
        transaction.commit().map_err(sqlite_error)?;
        Ok(commission)
    }

    fn delete(&self, id: &CommissionId, expected_revision: u64) -> Result<(), AppError> {
        let mut connection = self.database.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
        let deleted = transaction
            .execute(
                "DELETE FROM commissions WHERE id=?1 AND revision=?2",
                params![
                    id.0.to_string(),
                    i64::try_from(expected_revision).map_err(|_| storage_error(
                        "commission.revision.invalid",
                        "Die Commission-Version ist zu groß."
                    ))?
                ],
            )
            .map_err(sqlite_error)?;
        if deleted == 0 {
            return Err(AppError::validation(
                "commission.revision_conflict",
                "Die Commission fehlt oder wurde zwischenzeitlich geändert.",
            ));
        }
        let deleted_revision = expected_revision.checked_add(1).ok_or_else(|| {
            storage_error(
                "commission.revision.invalid",
                "Die Commission-Version ist zu groß.",
            )
        })?;
        let payload = serde_json::json!({"deleted": true, "expected_revision": expected_revision});
        record_change(
            &transaction,
            "commission_deleted",
            &id.0.to_string(),
            deleted_revision,
            &payload,
            Utc::now(),
        )?;
        transaction.commit().map_err(sqlite_error)
    }
}

impl ProjectRepository for SqliteProjectRepository {
    fn insert(&self, project: Project) -> Result<(), AppError> {
        let mut connection = self.database.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
        insert_project(&transaction, &project)?;
        record_change(
            &transaction,
            "project",
            &project.id.0.to_string(),
            project.revision,
            &project,
            project.created_at,
        )?;
        transaction.commit().map_err(sqlite_error)
    }

    fn list(&self) -> Result<Vec<Project>, AppError> {
        self.search(None, false)
    }

    fn get(&self, id: &ProjectId) -> Result<Option<Project>, AppError> {
        let connection = self.database.connection()?;
        connection.query_row("SELECT id,name,description,tags_json,created_at,updated_at,archived_at,revision FROM projects WHERE id=?1", [id.0.to_string()], row_to_project).optional().map_err(sqlite_error)
    }

    fn update(&self, mut project: Project, expected_revision: u64) -> Result<Project, AppError> {
        project.updated_at = Utc::now();
        let mut connection = self.database.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
        let expected_revision_db = i64::try_from(expected_revision).map_err(|_| {
            storage_error(
                "project.revision.invalid",
                "Die Projektversion ist zu groß.",
            )
        })?;
        let changed = transaction.execute("UPDATE projects SET name=?1, description=?2, tags_json=?3, updated_at=?4, archived_at=?5, revision=revision+1 WHERE id=?6 AND revision=?7", params![project.name.as_str(), project.description, serde_json::to_string(&project.tags).map_err(json_error)?, project.updated_at.to_rfc3339(), project.archived_at.map(|value| value.to_rfc3339()), project.id.0.to_string(), expected_revision_db]).map_err(sqlite_error)?;
        if changed == 0 {
            return Err(project_conflict_or_missing(
                &transaction,
                &project.id,
                expected_revision,
            )?);
        }
        project.revision = expected_revision + 1;
        record_change(
            &transaction,
            "project",
            &project.id.0.to_string(),
            project.revision,
            &project,
            project.updated_at,
        )?;
        transaction.commit().map_err(sqlite_error)?;
        Ok(project)
    }

    fn archive(
        &self,
        id: &ProjectId,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<Project, AppError> {
        let mut connection = self.database.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
        let expected_revision_db = i64::try_from(expected_revision).map_err(|_| {
            storage_error(
                "project.revision.invalid",
                "Die Projektversion ist zu groß.",
            )
        })?;
        let changed = transaction.execute("UPDATE projects SET archived_at=?1, updated_at=?1, revision=revision+1 WHERE id=?2 AND revision=?3", params![now.to_rfc3339(), id.0.to_string(), expected_revision_db]).map_err(sqlite_error)?;
        if changed == 0 {
            return Err(project_conflict_or_missing(
                &transaction,
                id,
                expected_revision,
            )?);
        }
        let project = transaction.query_row("SELECT id,name,description,tags_json,created_at,updated_at,archived_at,revision FROM projects WHERE id=?1", [id.0.to_string()], row_to_project).map_err(sqlite_error)?;
        record_change(
            &transaction,
            "project",
            &id.0.to_string(),
            project.revision,
            &project,
            now,
        )?;
        transaction.commit().map_err(sqlite_error)?;
        Ok(project)
    }

    fn search(
        &self,
        query: Option<&str>,
        include_archived: bool,
    ) -> Result<Vec<Project>, AppError> {
        let connection = self.database.connection()?;
        let sql = if include_archived {
            "SELECT id,name,description,tags_json,created_at,updated_at,archived_at,revision FROM projects WHERE (?1 IS NULL OR instr(lower(name),lower(?1))>0) ORDER BY created_at DESC"
        } else {
            "SELECT id,name,description,tags_json,created_at,updated_at,archived_at,revision FROM projects WHERE archived_at IS NULL AND (?1 IS NULL OR instr(lower(name),lower(?1))>0) ORDER BY created_at DESC"
        };
        let mut statement = connection.prepare(sql).map_err(sqlite_error)?;
        let projects = statement
            .query_map([query], row_to_project)
            .map_err(sqlite_error)?;
        projects
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite_error)
    }
}

fn migrate(connection: &Connection) -> Result<(), AppError> {
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(sqlite_error)?;
    if version > SCHEMA_VERSION {
        return Err(storage_error(
            "database.schema_newer",
            "Die Datenbank wurde mit einer neueren Apex Studio-Version erstellt.",
        ));
    }
    if version == 0 {
        let transaction = connection.unchecked_transaction().map_err(sqlite_error)?;
        transaction.execute_batch(SCHEMA_V1).map_err(sqlite_error)?;
        transaction.execute_batch(SCHEMA_V2).map_err(sqlite_error)?;
        transaction
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(sqlite_error)?;
        transaction.commit().map_err(sqlite_error)?;
    } else if version == 1 {
        let transaction = connection.unchecked_transaction().map_err(sqlite_error)?;
        transaction.execute_batch(SCHEMA_V2).map_err(sqlite_error)?;
        transaction
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(sqlite_error)?;
        transaction.commit().map_err(sqlite_error)?;
    }
    Ok(())
}

const SCHEMA_V1: &str = r#"
CREATE TABLE workspaces(id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT NOT NULL);
INSERT INTO workspaces(id,name,created_at) VALUES('local','Local Workspace',strftime('%Y-%m-%dT%H:%M:%fZ','now'));
CREATE TABLE projects(id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id), name TEXT NOT NULL, description TEXT, tags_json TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL, updated_at TEXT NOT NULL, archived_at TEXT, revision INTEGER NOT NULL CHECK(revision > 0));
CREATE INDEX projects_name_idx ON projects(name COLLATE NOCASE);
CREATE TABLE clients(id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE TABLE organisations(id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE TABLE designers(id TEXT PRIMARY KEY, handle TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE TABLE commissions(id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), display_id TEXT NOT NULL, name TEXT NOT NULL, client_id TEXT REFERENCES clients(id), client_name TEXT, organisation_id TEXT REFERENCES organisations(id), organisation_name TEXT, commission_type TEXT, description TEXT, requirements_json TEXT NOT NULL DEFAULT '[]', references_json TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL, due_date TEXT, status TEXT NOT NULL CHECK(status IN ('draft','active','on_hold','completed','delivered','cancelled')), priority TEXT CHECK(priority IS NULL OR priority IN ('low','normal','high','urgent')), assigned_designer_id TEXT REFERENCES designers(id), assigned_designer_handle TEXT, started_at TEXT, completed_at TEXT, delivered_at TEXT, revision_count INTEGER NOT NULL DEFAULT 0 CHECK(revision_count >= 0), internal_notes TEXT, design_type TEXT, vehicle_model TEXT, template_reference TEXT, figma_file_url TEXT, export_files_json TEXT NOT NULL DEFAULT '[]', roblox_identifiers_json TEXT NOT NULL DEFAULT '[]', file_hashes_json TEXT NOT NULL DEFAULT '[]', version TEXT, total_price_minor INTEGER, currency TEXT, currency_precision INTEGER, payment_status TEXT CHECK(payment_status IS NULL OR payment_status IN ('unpaid','partial','paid','refunded','waived')), payment_method TEXT, usage_rights TEXT, exclusivity INTEGER CHECK(exclusivity IS NULL OR exclusivity IN (0,1)), license_notes TEXT, payload_json TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0), UNIQUE(project_id, display_id), CHECK((total_price_minor IS NULL AND currency IS NULL AND currency_precision IS NULL) OR (total_price_minor >= 0 AND currency IS NOT NULL AND currency_precision BETWEEN 0 AND 4)));
CREATE INDEX commissions_project_idx ON commissions(project_id,created_at);
CREATE TABLE commission_counters(workspace_id TEXT NOT NULL REFERENCES workspaces(id), year INTEGER NOT NULL, next_number INTEGER NOT NULL CHECK(next_number > 0), PRIMARY KEY(workspace_id,year));
CREATE TABLE design_sets(id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), commission_id TEXT REFERENCES commissions(id), design_type TEXT, vehicle_model TEXT, template_reference TEXT, figma_file_url TEXT, version TEXT, created_at TEXT NOT NULL);
CREATE TABLE assets(id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), original_name TEXT NOT NULL, relative_path TEXT NOT NULL, side TEXT, slot_index INTEGER, width INTEGER, height INTEGER, byte_count INTEGER, imported_at TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 1);
CREATE TABLE asset_versions(id TEXT PRIMARY KEY, asset_id TEXT NOT NULL REFERENCES assets(id), version TEXT NOT NULL, imported_at TEXT NOT NULL, file_sha256 TEXT, pixel_sha256_v1 TEXT, phash_v1 TEXT, UNIQUE(asset_id,version));
CREATE TABLE reference_materials(id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), commission_id TEXT REFERENCES commissions(id), kind TEXT NOT NULL, label TEXT NOT NULL, uri TEXT NOT NULL, visible_to_client INTEGER NOT NULL DEFAULT 0);
CREATE TABLE notes(id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), commission_id TEXT REFERENCES commissions(id), title TEXT NOT NULL, relative_path TEXT NOT NULL, visibility TEXT NOT NULL DEFAULT 'private', revision INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE TABLE upload_jobs(id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), asset_version_id TEXT REFERENCES asset_versions(id), state TEXT NOT NULL, created_at TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 1);
CREATE TABLE upload_attempts(id TEXT PRIMARY KEY, upload_job_id TEXT NOT NULL REFERENCES upload_jobs(id), attempt INTEGER NOT NULL, operation_id TEXT, state TEXT NOT NULL, started_at TEXT, completed_at TEXT, error_code TEXT, UNIQUE(upload_job_id,attempt));
CREATE TABLE asset_identifiers(id TEXT PRIMARY KEY, asset_version_id TEXT NOT NULL REFERENCES asset_versions(id), kind TEXT NOT NULL CHECK(kind IN ('image','decal')), external_id TEXT, status TEXT NOT NULL, source TEXT, verified_at TEXT);
CREATE TABLE hash_records(id TEXT PRIMARY KEY, asset_version_id TEXT NOT NULL REFERENCES asset_versions(id), algorithm TEXT NOT NULL, algorithm_version INTEGER NOT NULL, value TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE TABLE settings(key TEXT PRIMARY KEY, value_json TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE TABLE revisions(id TEXT PRIMARY KEY, entity_type TEXT NOT NULL, entity_id TEXT NOT NULL, revision INTEGER NOT NULL, payload_json TEXT NOT NULL, created_at TEXT NOT NULL, UNIQUE(entity_type,entity_id,revision));
CREATE TABLE outbox(id TEXT PRIMARY KEY, entity_type TEXT NOT NULL, entity_id TEXT NOT NULL, revision INTEGER NOT NULL, payload_json TEXT NOT NULL, created_at TEXT NOT NULL, delivered_at TEXT, UNIQUE(entity_type,entity_id,revision));
"#;

const SCHEMA_V2: &str = r#"
CREATE TABLE upload_jobs_v2(id TEXT PRIMARY KEY, creator_profile_id TEXT NOT NULL, asset_version_id TEXT, upload_type TEXT NOT NULL CHECK(upload_type IN ('image','decal')), file_sha256 TEXT NOT NULL CHECK(length(file_sha256)=64), state TEXT NOT NULL CHECK(state IN ('queued','uploading','awaiting_operation','awaiting_moderation','succeeded','failed_retryable','failed_final','unknown_remote_outcome','cancel_requested','cancelled')), operation_id TEXT, snapshot_json TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0), lease_owner TEXT, lease_expires_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE INDEX upload_jobs_v2_state_idx ON upload_jobs_v2(state,created_at);
CREATE INDEX upload_jobs_v2_duplicate_idx ON upload_jobs_v2(creator_profile_id,upload_type,file_sha256);
CREATE TABLE upload_events(id TEXT PRIMARY KEY, job_id TEXT NOT NULL REFERENCES upload_jobs_v2(id), event_type TEXT NOT NULL, payload_json TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE INDEX upload_events_job_idx ON upload_events(job_id,created_at);
CREATE TABLE asset_identifier_preferences(project_id TEXT PRIMARY KEY, preferred_kind TEXT NOT NULL CHECK(preferred_kind IN ('image','decal')), copy_format TEXT NOT NULL CHECK(copy_format IN ('numeric','rbxassetid_uri')), updated_at TEXT NOT NULL);
CREATE TABLE asset_identifier_settings(id INTEGER PRIMARY KEY CHECK(id=1), preferred_kind TEXT NOT NULL CHECK(preferred_kind IN ('image','decal')), copy_format TEXT NOT NULL CHECK(copy_format IN ('numeric','rbxassetid_uri')), updated_at TEXT NOT NULL);
INSERT INTO asset_identifier_settings(id,preferred_kind,copy_format,updated_at) VALUES(1,'image','numeric',strftime('%Y-%m-%dT%H:%M:%fZ','now'));
"#;

fn ensure_party(
    transaction: &Transaction<'_>,
    table: &str,
    id: &mut Option<String>,
    name: &Option<String>,
) -> Result<(), AppError> {
    if name.is_some() && id.is_none() {
        *id = Some(Uuid::now_v7().to_string());
    }
    if let (Some(id), Some(name)) = (id.as_ref(), name.as_ref()) {
        let sql = match table {
            "clients" => {
                "INSERT INTO clients(id,name,created_at) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET name=excluded.name"
            }
            "organisations" => {
                "INSERT INTO organisations(id,name,created_at) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET name=excluded.name"
            }
            _ => {
                return Err(storage_error(
                    "database.party_table_invalid",
                    "Interner Tabellenfehler.",
                ));
            }
        };
        transaction
            .execute(sql, params![id, name, Utc::now().to_rfc3339()])
            .map_err(sqlite_error)?;
    }
    if let Some(id) = id.as_ref().filter(|_| name.is_none()) {
        let sql = match table {
            "clients" => "SELECT EXISTS(SELECT 1 FROM clients WHERE id=?1)",
            "organisations" => "SELECT EXISTS(SELECT 1 FROM organisations WHERE id=?1)",
            _ => {
                return Err(storage_error(
                    "database.party_table_invalid",
                    "Interner Tabellenfehler.",
                ));
            }
        };
        let exists: bool = transaction
            .query_row(sql, [id], |row| row.get(0))
            .map_err(sqlite_error)?;
        if !exists {
            return Err(AppError::validation(
                "commission.party.not_found",
                "Die verknüpfte Kunden- oder Organisationsangabe wurde nicht gefunden.",
            ));
        }
    }
    Ok(())
}

fn commission_sql_values(commission: &Commission, payload: &str) -> Result<Vec<Value>, AppError> {
    let text = |value: Option<String>| value.map(Value::Text).unwrap_or(Value::Null);
    let timestamp = |value: Option<DateTime<Utc>>| {
        value
            .map(|value| Value::Text(value.to_rfc3339()))
            .unwrap_or(Value::Null)
    };
    let priority = commission
        .priority
        .as_ref()
        .map(|value| Value::Text(format!("{value:?}").to_lowercase()))
        .unwrap_or(Value::Null);
    let payment_status = commission
        .payment_status
        .as_ref()
        .map(|value| Value::Text(format!("{value:?}").to_lowercase()))
        .unwrap_or(Value::Null);
    let precision = commission
        .currency_precision
        .map(|value| Value::Integer(i64::from(value)))
        .unwrap_or(Value::Null);
    let exclusivity = commission
        .exclusivity
        .map(|value| Value::Integer(i64::from(value)))
        .unwrap_or(Value::Null);
    Ok(vec![
        Value::Text(commission.id.0.to_string()),
        Value::Text(commission.project_id.0.to_string()),
        Value::Text(commission.display_id.clone()),
        Value::Text(commission.name.clone()),
        text(commission.client_id.clone()),
        text(commission.client_name.clone()),
        text(commission.organisation_id.clone()),
        text(commission.organisation_name.clone()),
        text(commission.commission_type.clone()),
        text(commission.description.clone()),
        json_value(&commission.requirements)?,
        json_value(&commission.reference_materials)?,
        Value::Text(commission.created_at.to_rfc3339()),
        text(commission.due_date.map(|value| value.to_string())),
        Value::Text(status_name(&commission.status).into()),
        priority,
        text(commission.assigned_designer_id.clone()),
        text(commission.assigned_designer_handle.clone()),
        timestamp(commission.started_at),
        timestamp(commission.completed_at),
        timestamp(commission.delivered_at),
        Value::Integer(i64::from(commission.revision_count)),
        text(commission.internal_notes.clone()),
        text(commission.design_type.clone()),
        text(commission.vehicle_model.clone()),
        text(commission.template_reference.clone()),
        text(commission.figma_file_url.clone()),
        json_value(&commission.export_files)?,
        json_value(&commission.roblox_identifiers)?,
        json_value(&commission.file_hashes)?,
        text(commission.version.clone()),
        commission
            .total_price_minor
            .map(Value::Integer)
            .unwrap_or(Value::Null),
        text(
            commission
                .currency
                .as_ref()
                .map(|value| value.as_str().to_owned()),
        ),
        precision,
        payment_status,
        text(commission.payment_method.clone()),
        text(commission.usage_rights.clone()),
        exclusivity,
        text(commission.license_notes.clone()),
        Value::Text(payload.to_owned()),
        Value::Integer(i64::try_from(commission.revision).map_err(|_| {
            storage_error(
                "commission.revision.invalid",
                "Die Commission-Version ist zu groß.",
            )
        })?),
    ])
}

fn json_value<T: serde::Serialize>(value: &T) -> Result<Value, AppError> {
    serde_json::to_string(value)
        .map(Value::Text)
        .map_err(json_error)
}

fn status_name(status: &CommissionStatus) -> &'static str {
    match status {
        CommissionStatus::Draft => "draft",
        CommissionStatus::Active => "active",
        CommissionStatus::OnHold => "on_hold",
        CommissionStatus::Completed => "completed",
        CommissionStatus::Delivered => "delivered",
        CommissionStatus::Cancelled => "cancelled",
    }
}

fn insert_project(transaction: &Transaction<'_>, project: &Project) -> Result<(), AppError> {
    let revision = i64::try_from(project.revision).map_err(|_| {
        storage_error(
            "project.revision.invalid",
            "Die Projektversion ist zu groß.",
        )
    })?;
    transaction.execute("INSERT INTO projects(id,workspace_id,name,description,tags_json,created_at,updated_at,archived_at,revision) VALUES(?1,'local',?2,?3,?4,?5,?6,?7,?8)", params![project.id.0.to_string(), project.name.as_str(), project.description, serde_json::to_string(&project.tags).map_err(json_error)?, project.created_at.to_rfc3339(), project.updated_at.to_rfc3339(), project.archived_at.map(|value| value.to_rfc3339()), revision]).map_err(sqlite_error)?;
    Ok(())
}

fn row_to_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let description: Option<String> = row.get(2)?;
    let tags_json: String = row.get(3)?;
    let created_at: String = row.get(4)?;
    let updated_at: String = row.get(5)?;
    let archived_at: Option<String> = row.get(6)?;
    let revision: i64 = row.get(7)?;
    let timestamp = |value: String| {
        DateTime::parse_from_rfc3339(&value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
    };
    let mut project = Project::with_id(
        ProjectId(Uuid::parse_str(&id).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?),
        ProjectName::parse(name).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::other(error.message)),
            )
        })?,
        timestamp(created_at.clone())?,
    );
    project.description = description;
    project.tags = serde_json::from_str(&tags_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(error))
    })?;
    project.updated_at = timestamp(updated_at)?;
    project.archived_at = archived_at.map(timestamp).transpose()?;
    project.revision = u64::try_from(revision).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    Ok(project)
}

fn project_conflict_or_missing(
    transaction: &Transaction<'_>,
    id: &ProjectId,
    expected: u64,
) -> Result<AppError, AppError> {
    let current: Option<i64> = transaction
        .query_row(
            "SELECT revision FROM projects WHERE id=?1",
            [id.0.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(sqlite_error)?;
    Ok(match current {
        Some(revision) => AppError {
            code: "project.revision_conflict".into(),
            message: "Das Projekt wurde zwischenzeitlich geändert.".into(),
            retryable: false,
            details: Some(format!(
                "expected_revision={expected};current_revision={revision}"
            )),
        },
        None => AppError::validation("project.not_found", "Das Projekt wurde nicht gefunden."),
    })
}

fn record_change<T: serde::Serialize>(
    transaction: &Transaction<'_>,
    kind: &str,
    id: &str,
    revision: u64,
    payload: &T,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    let revision = i64::try_from(revision)
        .map_err(|_| storage_error("entity.revision.invalid", "Die Version ist zu groß."))?;
    let payload = serde_json::to_string(payload).map_err(json_error)?;
    transaction.execute("INSERT INTO revisions(id,entity_type,entity_id,revision,payload_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)", params![Uuid::now_v7().to_string(), kind, id, revision, payload, now.to_rfc3339()]).map_err(sqlite_error)?;
    transaction.execute("INSERT INTO outbox(id,entity_type,entity_id,revision,payload_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)", params![Uuid::now_v7().to_string(), kind, id, revision, payload, now.to_rfc3339()]).map_err(sqlite_error)?;
    Ok(())
}

fn sqlite_error(error: rusqlite::Error) -> AppError {
    AppError {
        code: "storage.sqlite_error".into(),
        message: "Die lokale Datenbank konnte den Vorgang nicht abschließen.".into(),
        retryable: matches!(error, rusqlite::Error::SqliteFailure(ref failure, _) if failure.code == rusqlite::ErrorCode::DatabaseBusy || failure.code == rusqlite::ErrorCode::DatabaseLocked),
        details: None,
    }
}

fn json_error(error: serde_json::Error) -> AppError {
    storage_error("storage.serialization", error.to_string())
}
fn storage_error(code: &str, message: impl Into<String>) -> AppError {
    AppError {
        code: code.into(),
        message: message.into(),
        retryable: false,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apex_domain::{
        AssetIdentifierKind, CommissionIdentifier, CommissionPriority, CommissionReference,
        CurrencyCode, PaymentStatus, RelativePathRef,
    };
    use chrono::NaiveDate;

    fn project(name: &str) -> Project {
        let now = Utc::now();
        Project::with_id(ProjectId::new_v7(), ProjectName::parse(name).unwrap(), now)
    }

    fn full_commission(project_id: ProjectId) -> Commission {
        Commission {
            id: CommissionId::new_v7(),
            project_id,
            display_id: String::new(),
            name: "Winterflotte".into(),
            client_id: None,
            client_name: Some("Northstar RP".into()),
            organisation_id: None,
            organisation_name: Some("Apex Designs".into()),
            commission_type: Some("Livery".into()),
            description: Some("Seasonal fleet set".into()),
            requirements: vec!["Keep unit numbers readable".into()],
            reference_materials: vec![
                CommissionReference {
                    kind: "image".into(),
                    label: "Reference".into(),
                    uri: "https://example.invalid/reference.png".into(),
                    visible_to_client: true,
                },
                CommissionReference {
                    kind: "image".into(),
                    label: "Private reference".into(),
                    uri: "https://example.invalid/private.png".into(),
                    visible_to_client: false,
                },
            ],
            created_at: Utc::now(),
            due_date: Some(NaiveDate::from_ymd_opt(2026, 12, 1).unwrap()),
            status: CommissionStatus::Active,
            priority: Some(CommissionPriority::High),
            assigned_designer_id: None,
            assigned_designer_handle: None,
            started_at: None,
            completed_at: None,
            delivered_at: None,
            revision_count: 3,
            internal_notes: Some("Private production note".into()),
            design_type: Some("Emergency services".into()),
            vehicle_model: Some("Falcon Interceptor Utility 2024".into()),
            template_reference: Some("templates/falcon.png".into()),
            figma_file_url: Some("https://figma.com/file/example".into()),
            export_files: vec![RelativePathRef::parse("exports/left.png").unwrap()],
            roblox_identifiers: vec![CommissionIdentifier {
                kind: AssetIdentifierKind::Image,
                id: apex_domain::ExternalRobloxId::parse("900719925474099312345").unwrap(),
                source: "manual".into(),
            }],
            file_hashes: vec!["sha256:abc123".into()],
            version: Some("v2".into()),
            total_price_minor: Some(123456789012345),
            currency: Some(CurrencyCode::parse("EUR").unwrap()),
            currency_precision: Some(2),
            payment_status: Some(PaymentStatus::Partial),
            payment_method: Some("Robux".into()),
            usage_rights: Some("Single community".into()),
            exclusivity: Some(true),
            license_notes: Some("No resales".into()),
            revision: 0,
        }
    }

    #[test]
    fn project_survives_database_reopen_and_revision_conflicts_do_not_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let database = SqliteDatabase::open(&path).unwrap();
        let repository = SqliteProjectRepository::new(database.clone());
        let created = project("NYPD Flotte");
        repository.insert(created.clone()).unwrap();

        let mut changed = created.clone();
        changed.name = ProjectName::parse("NYPD Winter").unwrap();
        let updated = repository.update(changed.clone(), 1).unwrap();
        assert_eq!(updated.revision, 2);
        let stale = repository.update(created.clone(), 1).unwrap_err();
        assert_eq!(stale.code, "project.revision_conflict");
        assert_eq!(
            repository.get(&created.id).unwrap().unwrap().name.as_str(),
            "NYPD Winter"
        );

        drop(repository);
        drop(database);
        let reopened = SqliteProjectRepository::new(SqliteDatabase::open(&path).unwrap());
        assert_eq!(reopened.get(&created.id).unwrap().unwrap(), updated);
    }

    #[test]
    fn archive_is_reversible_by_filter_and_search_uses_bound_values() {
        let directory = tempfile::tempdir().unwrap();
        let repository = SqliteProjectRepository::new(
            SqliteDatabase::open(directory.path().join("db.sqlite3")).unwrap(),
        );
        let created = project("Fleet 100%_ready");
        repository.insert(created.clone()).unwrap();
        assert_eq!(repository.search(Some("100%_"), false).unwrap().len(), 1);
        let archived = repository.archive(&created.id, 1, Utc::now()).unwrap();
        assert!(repository.search(None, false).unwrap().is_empty());
        assert_eq!(repository.search(None, true).unwrap(), vec![archived]);
    }

    #[test]
    fn online_backup_includes_committed_wal_data_and_refuses_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join("live.sqlite3");
        let backup_path = directory.path().join("backup.sqlite3");
        let database = SqliteDatabase::open(&source_path).unwrap();
        let repository = SqliteProjectRepository::new(database.clone());
        let created = project("WAL Restore");
        repository.insert(created.clone()).unwrap();
        database.backup_to(&backup_path).unwrap();
        assert_eq!(
            database.backup_to(&backup_path).unwrap_err().code,
            "backup.destination_exists"
        );
        let restored = SqliteProjectRepository::new(SqliteDatabase::open(backup_path).unwrap());
        assert_eq!(restored.get(&created.id).unwrap(), Some(created));
    }

    #[test]
    fn newer_schema_is_refused_without_rewriting_the_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("newer.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        drop(connection);
        assert_eq!(
            SqliteDatabase::open(&path).err().unwrap().code,
            "database.schema_newer"
        );
        let connection = Connection::open(path).unwrap();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION + 1);
    }

    #[test]
    fn schema_v1_migrates_to_v2_and_keeps_preferences_available() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("v1.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        drop(connection);
        let database = SqliteDatabase::open(&path).unwrap();
        let version: i64 = database
            .connection()
            .unwrap()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        assert_eq!(
            database
                .identifier_preferences(None)
                .unwrap()
                .preferred_kind,
            RobloxIdentifierKind::Image
        );
    }

    #[test]
    fn upload_queue_uses_leases_and_preserves_unknown_remote_outcomes() {
        let directory = tempfile::tempdir().unwrap();
        let database = SqliteDatabase::open(directory.path().join("queue.sqlite3")).unwrap();
        let queued = database
            .enqueue_upload(
                "profile-a",
                None,
                "decal",
                &"a".repeat(64),
                r#"{"asset_version_id":"immutable-v1"}"#,
            )
            .unwrap();
        assert_eq!(queued.state, UploadState::Queued);
        let claimed = database.claim_upload("desktop", 30).unwrap().unwrap();
        assert_eq!(claimed.state, UploadState::Uploading);
        assert_eq!(claimed.id, queued.id);
        assert!(database.claim_upload("cli", 30).unwrap().is_none());
        let unknown = database
            .mark_upload_unknown(&claimed.id, "desktop")
            .unwrap();
        assert_eq!(unknown.state, UploadState::UnknownRemoteOutcome);
        assert!(unknown.operation_id.is_none());
        assert!(database.claim_upload("cli", 30).unwrap().is_none());
        drop(database);
        let reopened = SqliteDatabase::open(directory.path().join("queue.sqlite3")).unwrap();
        let history = reopened.upload_history(10).unwrap();
        assert_eq!(history[0].state, UploadState::UnknownRemoteOutcome);
        assert_eq!(
            history[0].snapshot_json,
            r#"{"asset_version_id":"immutable-v1"}"#
        );
    }

    #[test]
    fn upload_operation_is_persisted_before_lease_release_and_bad_ids_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let database = SqliteDatabase::open(directory.path().join("queue.sqlite3")).unwrap();
        let _ = database
            .enqueue_upload("profile-a", None, "image", &"b".repeat(64), "{}")
            .unwrap();
        let claimed = database.claim_upload("worker", 20).unwrap().unwrap();
        assert_eq!(
            database
                .save_upload_operation(&claimed.id, "wrong-worker", "op-1")
                .unwrap_err()
                .code,
            "upload.lease_conflict"
        );
        assert_eq!(
            database
                .save_upload_operation(&claimed.id, "worker", "../bad")
                .unwrap_err()
                .code,
            "upload.operation.invalid"
        );
        let awaiting = database
            .save_upload_operation(&claimed.id, "worker", "operation_123")
            .unwrap();
        assert_eq!(awaiting.state, UploadState::AwaitingOperation);
        assert_eq!(awaiting.operation_id.as_deref(), Some("operation_123"));
        assert!(database.claim_upload("other", 20).unwrap().is_none());
    }

    #[test]
    fn global_and_project_identifier_preferences_survive_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("prefs.sqlite3");
        let database = SqliteDatabase::open(&path).unwrap();
        let project = ProjectId::new_v7();
        database
            .save_identifier_preferences(
                None,
                IdentifierPreferences {
                    preferred_kind: RobloxIdentifierKind::Decal,
                    copy_format: CopyIdFormat::RbxassetidUri,
                },
            )
            .unwrap();
        database
            .save_identifier_preferences(
                Some(&project),
                IdentifierPreferences {
                    preferred_kind: RobloxIdentifierKind::Image,
                    copy_format: CopyIdFormat::Numeric,
                },
            )
            .unwrap();
        drop(database);
        let reopened = SqliteDatabase::open(path).unwrap();
        assert_eq!(
            reopened
                .identifier_preferences(None)
                .unwrap()
                .preferred_kind,
            RobloxIdentifierKind::Decal
        );
        assert_eq!(
            reopened.identifier_preferences(None).unwrap().copy_format,
            CopyIdFormat::RbxassetidUri
        );
        assert_eq!(
            reopened
                .identifier_preferences(Some(&project))
                .unwrap()
                .preferred_kind,
            RobloxIdentifierKind::Image
        );
    }

    #[test]
    fn upload_snapshot_rejects_secret_and_unapproved_fields() {
        let directory = tempfile::tempdir().unwrap();
        let database = SqliteDatabase::open(directory.path().join("queue.sqlite3")).unwrap();
        assert_eq!(
            database
                .enqueue_upload(
                    "profile",
                    None,
                    "image",
                    &"c".repeat(64),
                    r#"{"api_key":"not-a-secret"}"#
                )
                .unwrap_err()
                .code,
            "upload.snapshot.field_not_allowed"
        );
        assert_eq!(
            database
                .enqueue_upload(
                    "profile",
                    None,
                    "image",
                    &"c".repeat(64),
                    r#"{"file_sha256":"abc","relative_path":"C:\\Users\\private.png"}"#
                )
                .unwrap_err()
                .code,
            "upload.snapshot.field_not_allowed"
        );
    }

    #[test]
    fn commissions_roundtrip_private_and_financial_metadata_with_stable_ids() {
        let directory = tempfile::tempdir().unwrap();
        let database = SqliteDatabase::open(directory.path().join("commissions.sqlite3")).unwrap();
        let projects = SqliteProjectRepository::new(database.clone());
        let project = project("Winter Ops");
        projects.insert(project.clone()).unwrap();
        let commissions = SqliteCommissionRepository::new(database.clone());
        let mut invalid_timeline = full_commission(project.id.clone());
        invalid_timeline.delivered_at =
            Some(invalid_timeline.created_at - chrono::Duration::days(1));
        assert_eq!(
            invalid_timeline.validate().unwrap_err().code,
            "commission.workflow.date_order"
        );
        let created = commissions
            .create(full_commission(project.id.clone()))
            .unwrap();
        assert!(created.display_id.starts_with("COM-2026-"));
        assert_eq!(created.revision, 1);
        assert!(created.client_id.is_some());
        assert!(created.organisation_id.is_some());
        assert_eq!(commissions.get(&created.id).unwrap(), Some(created.clone()));
        assert_eq!(
            commissions.list_for_project(&project.id).unwrap(),
            vec![created.clone()]
        );
        let public_json = serde_json::to_value(created.client_share()).unwrap();
        assert!(public_json.get("internal_notes").is_none());
        assert!(public_json.get("total_price_minor").is_none());
        assert!(public_json.get("payment_method").is_none());
        assert_eq!(public_json["references"].as_array().unwrap().len(), 1);

        let mut changed = created.clone();
        changed.license_notes = Some("Updated licence".into());
        changed.total_price_minor = Some(123456789012346);
        let updated = commissions.update(changed.clone(), 1).unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(commissions.get(&created.id).unwrap(), Some(updated.clone()));
        assert_eq!(
            commissions.update(created.clone(), 1).unwrap_err().code,
            "commission.revision_conflict"
        );
        assert_eq!(commissions.get(&created.id).unwrap(), Some(updated));

        let mut unknown_price = full_commission(project.id.clone());
        unknown_price.id = CommissionId::new_v7();
        unknown_price.name = "Unknown price".into();
        unknown_price.client_id = None;
        unknown_price.client_name = None;
        unknown_price.organisation_id = None;
        unknown_price.organisation_name = None;
        unknown_price.total_price_minor = None;
        unknown_price.currency = None;
        unknown_price.currency_precision = None;
        unknown_price.due_date = None;
        let unknown_price = commissions.create(unknown_price).unwrap();
        assert_eq!(unknown_price.total_price_minor, None);
        assert_eq!(unknown_price.due_date, None);
        assert_eq!(unknown_price.display_id, "COM-2026-0002");

        let mut free = full_commission(project.id);
        free.id = CommissionId::new_v7();
        free.name = "Explicitly free".into();
        free.total_price_minor = Some(0);
        free.currency = Some(apex_domain::CurrencyCode::parse("JPY").unwrap());
        free.currency_precision = Some(0);
        let free = commissions.create(free).unwrap();
        assert_eq!(free.total_price_minor, Some(0));
        assert_eq!(free.currency_precision, Some(0));
    }
}
