//! Local-only sync bindings and atomic, project-scoped snapshots. Credentials never enter snapshots.
use super::{storage, Workspace};
use apex_core::Error;
use rusqlite::{params, types::Value as SqlValue, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSync {
    pub project_id: String,
    pub origin: String,
    pub server_instance: String,
    pub team_id: String,
    pub enabled: bool,
    pub received_from_team: bool,
    pub local_hash: Option<String>,
    pub revision: i64,
    pub cursor: i64,
    pub message: String,
    pub synced_at: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    const DEST: (&str, &str, &str) = ("https://sync.example.test", "instance", "team");
    fn project(ws: &Workspace) -> apex_core::Project {
        ws.save_project(
            None,
            apex_core::ProjectInput {
                name: "Shared design".into(),
                ..Default::default()
            },
        )
        .unwrap()
    }
    #[test]
    fn snapshots_transfer_files_folders_notes_and_versions_without_device_settings() {
        let temp = tempfile::tempdir().unwrap();
        let a = Workspace::open(temp.path().join("a")).unwrap();
        let b = Workspace::open(temp.path().join("b")).unwrap();
        let project = project(&a);
        let parent = a.save_folder(&project.id, None, None, "Liveries").unwrap();
        let vehicle = apex_core::vehicle_catalogue().unwrap().vehicles.remove(0);
        let png = temp.path().join("Left1.png");
        image::RgbImage::new(2, 2).save(&png).unwrap();
        a.import_livery(
            &project.id,
            Some(&parent.id),
            None,
            "Patrol",
            &vehicle.id,
            &[(png.clone(), "Left1.png".into(), Some("Left 1".into()))],
            Some(0),
        )
        .unwrap();
        let note = a.create_note(Some(&project.id)).unwrap();
        a.attach_note_image(&note.id, &png).unwrap();
        a.configure_project_sync(&project.id, Some(DEST)).unwrap();
        let before = a.project_snapshot(&project.id).unwrap();
        assert_eq!(
            before.fingerprint().unwrap(),
            a.project_snapshot(&project.id)
                .unwrap()
                .fingerprint()
                .unwrap()
        );
        for blob in &before.blobs {
            b.receive_sync_blob(&a.sync_blob_path(&blob.hash).unwrap(), &blob.hash)
                .unwrap();
        }
        b.apply_project_snapshot(&before, None, DEST, 2).unwrap();
        assert_eq!(before, b.project_snapshot(&project.id).unwrap());
        assert!(b.project_sync(&project.id).unwrap().unwrap().enabled);
        assert!(
            !a.project_sync(&project.id)
                .unwrap()
                .unwrap()
                .received_from_team
        );
        assert!(
            b.project_sync(&project.id)
                .unwrap()
                .unwrap()
                .received_from_team
        );
        assert!(serde_json::to_string(&before)
            .unwrap()
            .find("server_instance")
            .is_none());
        let old_hash = before.fingerprint().unwrap();
        let mut fields = project.fields.clone();
        fields.name = "Updated on A".into();
        a.save_project(Some(&project.id), fields).unwrap();
        let updated = a.project_snapshot(&project.id).unwrap();
        b.apply_project_snapshot(&updated, Some(&old_hash), DEST, 3)
            .unwrap();
        assert!(
            b.project_sync(&project.id)
                .unwrap()
                .unwrap()
                .received_from_team
        );
        assert_eq!(b.project(&project.id).unwrap().fields.name, "Updated on A");
        assert_eq!(b.projects().unwrap().len(), 1);
        b.configure_project_sync(&project.id, None).unwrap();
        assert!(
            b.project_sync(&project.id)
                .unwrap()
                .unwrap()
                .received_from_team
        );
    }
    #[test]
    fn concurrent_edits_disabled_sync_and_cross_project_rows_never_overwrite_local_data() {
        let temp = tempfile::tempdir().unwrap();
        let ws = Workspace::open(temp.path()).unwrap();
        let p = project(&ws);
        let other = project(&ws);
        ws.configure_project_sync(&p.id, Some(DEST)).unwrap();
        let incoming = ws.project_snapshot(&p.id).unwrap();
        let original = incoming.fingerprint().unwrap();
        let mut fields = p.fields.clone();
        fields.description = "Unsynced local change".into();
        ws.save_project(Some(&p.id), fields).unwrap();
        assert!(matches!(
            ws.apply_project_snapshot(&incoming, Some(&original), DEST, 2),
            Err(Error::Conflict)
        ));
        let current = ws.project_snapshot(&p.id).unwrap().fingerprint().unwrap();
        let mut malicious = incoming.clone();
        malicious.tables[0][0][0] = Value::String(other.id.clone());
        assert!(ws
            .apply_project_snapshot(&malicious, Some(&current), DEST, 2)
            .is_err());
        assert_eq!(
            ws.project(&p.id).unwrap().fields.description,
            "Unsynced local change"
        );
        assert!(ws.project(&other.id).is_ok());
        let mut foreign_note = incoming.clone();
        foreign_note.tables[6].push(vec![
            Value::from("foreign-note"),
            Value::from(other.id.clone()),
            Value::from("Do not insert"),
            Value::from("{}"),
            Value::from(""),
            Value::from(1),
            Value::from(0),
            Value::from("2026-11-03"),
            Value::from("2026-11-03"),
        ]);
        assert!(ws
            .apply_project_snapshot(&foreign_note, Some(&current), DEST, 2)
            .is_err());
        assert!(ws.notes(Some(&other.id)).unwrap().is_empty());
        assert_eq!(
            ws.project(&p.id).unwrap().fields.description,
            "Unsynced local change"
        );
        ws.configure_project_sync(&p.id, None).unwrap();
        assert!(ws
            .apply_project_snapshot(&incoming, Some(&current), DEST, 2)
            .is_err());
        assert!(!ws.project_sync(&p.id).unwrap().unwrap().enabled);
        drop(ws);
        let reopened = Workspace::open(temp.path()).unwrap();
        assert!(!reopened.project_sync(&p.id).unwrap().unwrap().enabled);
    }
}

// Names and columns come only from this fixed allowlist, never from a server response.
const TABLES: &[(&str, &str, &str)] = &[
    (
        "projects",
        "id,metadata,created_at,updated_at,archived",
        "id=?1",
    ),
    (
        "trash_entries",
        "id,project_id,root_id,kind,name,parent_id,deleted_at",
        "project_id=?1",
    ),
    (
        "asset_folders",
        "id,project_id,parent_id,name,kind,vehicle_id,cover_hash,trash_id",
        "project_id=?1",
    ),
    (
        "assets",
        "id,project_id,name,hash,imported_at,trash_id",
        "project_id=?1",
    ),
    (
        "asset_placements",
        "asset_id,folder_id,slot",
        "asset_id IN (SELECT id FROM assets WHERE project_id=?1)",
    ),
    (
        "asset_versions",
        "id,asset_id,hash,name,created_at",
        "asset_id IN (SELECT id FROM assets WHERE project_id=?1)",
    ),
    (
        "notes",
        "id,project_id,title,document,markdown,revision,archived,created_at,updated_at",
        "project_id=?1",
    ),
    (
        "note_images",
        "note_id,hash",
        "note_id IN (SELECT id FROM notes WHERE project_id=?1)",
    ),
    (
        "publishing_drafts",
        "project_id,data,revision",
        "project_id=?1",
    ),
    (
        "publishing_media",
        "project_id,hash,name,ordinal",
        "project_id=?1",
    ),
    (
        "posts",
        "id,data,revision,created_at,updated_at,legacy_project_id",
        "legacy_project_id=?1",
    ),
    (
        "post_media",
        "post_id,hash,name,ordinal",
        "post_id IN (SELECT id FROM posts WHERE legacy_project_id=?1)",
    ),
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncBlob {
    pub hash: String,
    pub size: u64,
    #[serde(default)]
    pub remote_only: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSnapshot {
    pub version: u32,
    pub project_id: String,
    pub tables: Vec<Vec<Vec<Value>>>,
    pub blobs: Vec<SyncBlob>,
}
impl ProjectSnapshot {
    pub fn validate(&self) -> Result<(), Error> {
        if self.version != 1
            || self.tables.len() != TABLES.len()
            || self.tables.iter().map(Vec::len).sum::<usize>() > 30_000
            || self.blobs.len() > 10_000
            || self
                .blobs
                .iter()
                .any(|b| !apex_core::valid_hash(&b.hash) || b.size > 512 * 1024 * 1024)
            || self
                .blobs
                .iter()
                .try_fold(0u64, |sum, b| sum.checked_add(b.size))
                .is_none_or(|size| size > 2 * 1024 * 1024 * 1024)
            || self.project_id.len() > 100
            || self.name()?.is_empty()
            || self.tables[0].len() != 1
            || self.tables[0][0].first().and_then(Value::as_str) != Some(self.project_id.as_str())
        {
            return Err(Error::InvalidContent(
                "Unsupported project sync snapshot.".into(),
            ));
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<String, Error> {
        // Delivery policy is transport metadata and must not turn an otherwise
        // unchanged pre-policy snapshot into a user-edit conflict.
        let mut canonical = self.clone();
        canonical
            .blobs
            .iter_mut()
            .for_each(|blob| blob.remote_only = false);
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical).map_err(storage)?)
        ))
    }
    pub fn classify_server_only_liveries(&mut self) {
        let mut livery_folders = std::collections::HashSet::new();
        let mut livery_hashes = std::collections::HashSet::new();
        if let Some(folders) = self.tables.get(2) {
            for folder in folders {
                if folder.get(4).and_then(Value::as_str) == Some("livery") {
                    if let Some(id) = folder.first().and_then(Value::as_str) {
                        livery_folders.insert(id.to_owned());
                    }
                    if let Some(hash) = folder.get(6).and_then(Value::as_str) {
                        livery_hashes.insert(hash.to_owned());
                    }
                }
            }
        }
        let mut livery_assets = std::collections::HashSet::new();
        if let (Some(assets), Some(placements)) = (self.tables.get(3), self.tables.get(4)) {
            let asset_folders = placements
                .iter()
                .filter_map(|row| {
                    Some((
                        row.first()?.as_str()?.to_owned(),
                        row.get(1)?.as_str()?.to_owned(),
                    ))
                })
                .collect::<std::collections::HashMap<_, _>>();
            for asset in assets {
                let (Some(id), Some(hash)) = (
                    asset.first().and_then(Value::as_str),
                    asset.get(3).and_then(Value::as_str),
                ) else {
                    continue;
                };
                if asset_folders
                    .get(id)
                    .is_some_and(|folder| livery_folders.contains(folder))
                {
                    livery_assets.insert(id.to_owned());
                    livery_hashes.insert(hash.to_owned());
                }
            }
        }
        if let Some(versions) = self.tables.get(5) {
            for version in versions {
                if version
                    .get(1)
                    .and_then(Value::as_str)
                    .is_some_and(|asset| livery_assets.contains(asset))
                {
                    if let Some(hash) = version.get(2).and_then(Value::as_str) {
                        livery_hashes.insert(hash.to_owned());
                    }
                }
            }
        }
        for blob in &mut self.blobs {
            blob.remote_only = livery_hashes.contains(&blob.hash);
        }
    }
    pub fn name(&self) -> Result<String, Error> {
        let metadata = self
            .tables
            .first()
            .and_then(|r| r.first())
            .and_then(|r| r.get(1))
            .and_then(Value::as_str)
            .ok_or(Error::NotFound)?;
        let fields: apex_core::ProjectInput = serde_json::from_str(metadata).map_err(storage)?;
        Ok(apex_core::validate_project(fields)?.name)
    }
}
fn snapshot(db: &Connection, id: &str) -> Result<ProjectSnapshot, Error> {
    let mut tables = Vec::new();
    for (name, columns, filter) in TABLES {
        let mut query = db
            .prepare(&format!(
                "SELECT {columns} FROM {name} WHERE {filter} ORDER BY {columns}"
            ))
            .map_err(storage)?;
        let count = query.column_count();
        let rows = query
            .query_map([id], |row| {
                (0..count)
                    .map(|i| {
                        Ok(match row.get::<_, SqlValue>(i)? {
                            SqlValue::Null => Value::Null,
                            SqlValue::Integer(v) => Value::from(v),
                            SqlValue::Text(v) => Value::from(v),
                            _ => return Err(rusqlite::Error::InvalidQuery),
                        })
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        tables.push(rows);
    }
    if tables[0].len() != 1 {
        return Err(Error::NotFound);
    }
    let mut query = db.prepare("SELECT hash,size,
        CASE WHEN EXISTS (SELECT 1 FROM assets a JOIN asset_placements p ON p.asset_id=a.id JOIN asset_folders f ON f.id=p.folder_id WHERE a.hash=managed_files.hash AND f.kind='livery' AND a.trash_id IS NULL AND f.trash_id IS NULL)
          OR EXISTS (SELECT 1 FROM asset_versions v JOIN assets a ON a.id=v.asset_id JOIN asset_placements p ON p.asset_id=a.id JOIN asset_folders f ON f.id=p.folder_id WHERE v.hash=managed_files.hash AND f.kind='livery' AND a.trash_id IS NULL AND f.trash_id IS NULL)
          OR EXISTS (SELECT 1 FROM asset_folders f WHERE f.cover_hash=managed_files.hash AND f.kind='livery' AND f.trash_id IS NULL)
        THEN 1 ELSE 0 END
        FROM managed_files WHERE hash IN (
        SELECT hash FROM assets WHERE project_id=?1
        UNION SELECT v.hash FROM asset_versions v JOIN assets a ON a.id=v.asset_id WHERE a.project_id=?1
        UNION SELECT cover_hash FROM asset_folders WHERE project_id=?1
        UNION SELECT i.hash FROM note_images i JOIN notes n ON n.id=i.note_id WHERE n.project_id=?1
        UNION SELECT hash FROM publishing_media WHERE project_id=?1
        UNION SELECT m.hash FROM post_media m JOIN posts p ON p.id=m.post_id WHERE p.legacy_project_id=?1
    ) ORDER BY hash").map_err(storage)?;
    let blobs = query
        .query_map([id], |r| {
            Ok(SyncBlob {
                hash: r.get(0)?,
                size: u64::try_from(r.get::<_, i64>(1)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                remote_only: r.get::<_, bool>(2)?,
            })
        })
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)?;
    Ok(ProjectSnapshot {
        version: 1,
        project_id: id.into(),
        tables,
        blobs,
    })
}
impl Workspace {
    pub fn project_sync(&self, id: &str) -> Result<Option<ProjectSync>, Error> {
        Ok(self
            .project_sync_bindings()?
            .into_iter()
            .find(|b| b.project_id == id))
    }
    pub fn project_sync_bindings(&self) -> Result<Vec<ProjectSync>, Error> {
        let db = self.connection()?;
        let mut q = db.prepare("SELECT project_id,origin,server_instance,team_id,enabled,local_hash,revision,message,synced_at,cursor,received_from_team FROM project_sync ORDER BY project_id").map_err(storage)?;
        let rows = q
            .query_map([], |r| {
                Ok(ProjectSync {
                    project_id: r.get(0)?,
                    origin: r.get(1)?,
                    server_instance: r.get(2)?,
                    team_id: r.get(3)?,
                    enabled: r.get(4)?,
                    local_hash: r.get(5)?,
                    revision: r.get(6)?,
                    message: r.get(7)?,
                    synced_at: r.get(8)?,
                    cursor: r.get(9)?,
                    received_from_team: r.get(10)?,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        Ok(rows)
    }
    pub fn configure_project_sync(
        &self,
        id: &str,
        destination: Option<(&str, &str, &str)>,
    ) -> Result<(), Error> {
        let project = self.project(id)?;
        if destination.is_some() && project.fields.name.chars().count() < 2 {
            return Err(Error::InvalidProject(
                "Use a project name with at least two characters to synchronize it.".into(),
            ));
        }
        let db = self.connection()?;
        if let Some((origin, instance, team)) = destination {
            if let Some(old) = self.project_sync(id)? {
                if old.origin != origin || old.server_instance != instance || old.team_id != team {
                    return Err(Error::InvalidContent("This project is linked to another team server. Create a project copy to share it with a different team.".into()));
                }
            }
            db.execute("INSERT INTO project_sync(project_id,origin,server_instance,team_id) VALUES(?1,?2,?3,?4) ON CONFLICT(project_id) DO UPDATE SET enabled=1,message='Waiting to sync'",params![id,origin,instance,team]).map_err(storage)?;
        } else {
            db.execute(
                "UPDATE project_sync SET enabled=0,message='Sync disabled' WHERE project_id=?1",
                [id],
            )
            .map_err(storage)?;
        }
        Ok(())
    }
    pub fn sync_message(&self, id: &str, message: &str) -> Result<(), Error> {
        self.connection()?
            .execute(
                "UPDATE project_sync SET message=?2 WHERE project_id=?1",
                params![id, message],
            )
            .map_err(storage)?;
        Ok(())
    }
    pub fn acknowledge_project_sync(
        &self,
        id: &str,
        hash: &str,
        revision: i64,
        cursor: i64,
    ) -> Result<(), Error> {
        self.connection()?.execute("UPDATE project_sync SET local_hash=?2,revision=?3,cursor=?4,message='Synced',synced_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE project_id=?1 AND enabled=1",params![id,hash,revision,cursor]).map_err(storage)?;
        Ok(())
    }
    pub fn project_snapshot(&self, id: &str) -> Result<ProjectSnapshot, Error> {
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(storage)?;
        snapshot(&tx, id)
    }
    pub fn livery_decal_codes(
        &self,
        project_id: &str,
        account_id: &str,
    ) -> Result<Vec<(String, Option<String>)>, Error> {
        let db = self.connection()?;
        let mut query = db.prepare(
            "SELECT DISTINCT a.hash,u.asset_id FROM assets a
             JOIN asset_placements p ON p.asset_id=a.id
             JOIN asset_folders f ON f.id=p.folder_id AND f.kind='livery'
             LEFT JOIN roblox_uploads u ON u.hash=a.hash AND u.account_id=?2 AND u.state='uploaded' AND u.moderation!='rejected'
             WHERE a.project_id=?1 AND a.trash_id IS NULL AND f.trash_id IS NULL ORDER BY a.hash",
        ).map_err(storage)?;
        let rows = query
            .query_map(params![project_id, account_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        Ok(rows)
    }
    pub fn save_shared_decal_code(
        &self,
        project_id: &str,
        team_id: &str,
        hash: &str,
        decal_asset_id: &str,
    ) -> Result<(), Error> {
        if !apex_core::valid_hash(hash)
            || decal_asset_id.is_empty()
            || decal_asset_id.len() > 20
            || decal_asset_id.starts_with('0')
            || !decal_asset_id.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(Error::InvalidContent("Invalid shared decal code.".into()));
        }
        let db = self.connection()?;
        let transaction = db.unchecked_transaction().map_err(storage)?;
        let existing = transaction.query_row(
            "SELECT team_id,decal_asset_id FROM shared_decal_codes WHERE project_id=?1 AND hash=?2",
            params![project_id, hash],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional().map_err(storage)?;
        if let Some((existing_team, existing_id)) = existing {
            if existing_team != team_id || existing_id != decal_asset_id {
                return Err(Error::Conflict);
            }
        } else {
            transaction.execute(
                "INSERT INTO shared_decal_codes(project_id,team_id,hash,decal_asset_id) VALUES(?1,?2,?3,?4)",
                params![project_id, team_id, hash, decal_asset_id],
            ).map_err(storage)?;
        }
        transaction.commit().map_err(storage)
    }
    pub fn clear_shared_decal_codes(&self, team_id: &str) -> Result<(), Error> {
        self.connection()?
            .execute("DELETE FROM shared_decal_codes WHERE team_id=?1", [team_id])
            .map_err(storage)?;
        Ok(())
    }
    pub fn sync_blob_path(&self, hash: &str) -> Result<PathBuf, Error> {
        if !apex_core::valid_hash(hash) {
            return Err(Error::InvalidContent("Invalid sync file hash.".into()));
        }
        Ok(self.root.join("files").join(hash))
    }
    pub fn receive_sync_blob(&self, path: &Path, expected: &str) -> Result<(), Error> {
        if !apex_core::valid_hash(expected) {
            return Err(Error::InvalidContent("Invalid sync file hash.".into()));
        }
        // Check before registering a downloaded file in the local content store.
        let mut file = std::fs::File::open(path).map_err(storage)?;
        let mut digest = Sha256::new();
        std::io::copy(&mut file, &mut digest).map_err(storage)?;
        if format!("{:x}", digest.finalize()) != expected {
            return Err(Error::InvalidContent(
                "A downloaded project file failed its integrity check.".into(),
            ));
        }
        self.import_file(path)?;
        Ok(())
    }
    pub fn remote_livery_blob(&self, project_id: &str, hash: &str) -> Result<bool, Error> {
        self.connection()?.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM assets a JOIN asset_placements p ON p.asset_id=a.id JOIN asset_folders f ON f.id=p.folder_id
                WHERE a.project_id=?1 AND a.hash=?2 AND f.kind='livery' AND a.trash_id IS NULL AND f.trash_id IS NULL
             ) OR EXISTS (
                SELECT 1 FROM asset_versions v JOIN assets a ON a.id=v.asset_id JOIN asset_placements p ON p.asset_id=a.id JOIN asset_folders f ON f.id=p.folder_id
                WHERE a.project_id=?1 AND v.hash=?2 AND f.kind='livery' AND a.trash_id IS NULL AND f.trash_id IS NULL
             ) OR EXISTS (
                SELECT 1 FROM asset_folders f WHERE f.project_id=?1 AND f.cover_hash=?2 AND f.kind='livery' AND f.trash_id IS NULL
             )",
            params![project_id, hash],
            |row| row.get(0),
        ).map_err(storage)
    }
    pub fn backup_before_sync_resolution(&self) -> Result<PathBuf, Error> {
        let path = self.root.join("backups").join(format!(
            "before-sync-resolution-{}.apexbackup",
            uuid::Uuid::now_v7()
        ));
        self.backup(&path)?;
        Ok(path)
    }
    /// Apply only to an absent project or to the exact snapshot observed before the network request.
    pub fn apply_project_snapshot(
        &self,
        incoming: &ProjectSnapshot,
        expected: Option<&str>,
        destination: (&str, &str, &str),
        revision: i64,
    ) -> Result<(), Error> {
        incoming.validate()?;
        for blob in &incoming.blobs {
            if !blob.remote_only
                && std::fs::metadata(self.sync_blob_path(&blob.hash)?)
                    .map_err(storage)?
                    .len()
                    != blob.size
            {
                return Err(Error::InvalidContent(
                    "A project file is missing or incomplete.".into(),
                ));
            }
        }
        let gate = self.content_gate();
        let _content = gate.lock();
        let mut db = self.connection()?;
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let present: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                [&incoming.project_id],
                |r| r.get(0),
            )
            .map_err(storage)?;
        match (present, expected) {
            (true, Some(hash)) if snapshot(&tx, &incoming.project_id)?.fingerprint()? == hash => {}
            (false, None) => {}
            _ => return Err(Error::Conflict),
        }
        if let Some(binding) = self.project_sync(&incoming.project_id)? {
            if !binding.enabled
                || binding.origin != destination.0
                || binding.server_instance != destination.1
                || binding.team_id != destination.2
            {
                return Err(Error::Conflict);
            }
        }
        tx.execute_batch("PRAGMA defer_foreign_keys=ON")
            .map_err(storage)?;
        for blob in &incoming.blobs {
            tx.execute(
                "INSERT OR IGNORE INTO managed_files(hash,size,local_available) VALUES(?1,?2,?3)",
                params![
                    blob.hash,
                    i64::try_from(blob.size).map_err(storage)?,
                    if blob.remote_only { 0 } else { 1 }
                ],
            )
            .map_err(storage)?;
        }
        // Child selections must run before their owning rows are removed.
        for i in [11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0] {
            let (table, _, filter) = TABLES[i];
            tx.execute(
                &format!("DELETE FROM {table} WHERE {filter}"),
                [&incoming.project_id],
            )
            .map_err(storage)?;
        }
        for ((table, columns, _), rows) in TABLES.iter().zip(&incoming.tables) {
            let count = columns.split(',').count();
            let placeholders = vec!["?"; count].join(",");
            for row in rows {
                if row.len() != count {
                    return Err(Error::InvalidContent("Invalid project sync record.".into()));
                }
                let values = row
                    .iter()
                    .map(|v| match v {
                        Value::Null => Ok(SqlValue::Null),
                        Value::String(s) => Ok(SqlValue::Text(s.clone())),
                        Value::Number(n) if n.as_i64().is_some() => {
                            Ok(SqlValue::Integer(n.as_i64().ok_or(Error::Conflict)?))
                        }
                        _ => Err(Error::InvalidContent("Invalid project sync value.".into())),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                tx.execute(
                    &format!("INSERT INTO {table}({columns}) VALUES({placeholders})"),
                    rusqlite::params_from_iter(values),
                )
                .map_err(storage)?;
            }
        }
        // Reject cross-project rows and references, including references to undisclosed local files.
        if snapshot(&tx, &incoming.project_id)? != *incoming {
            return Err(Error::InvalidContent(
                "The server project contains records outside its project.".into(),
            ));
        }
        tx.execute("INSERT INTO project_sync(project_id,origin,server_instance,team_id,local_hash,revision,message,synced_at,received_from_team) VALUES(?1,?2,?3,?4,?5,?6,'Synced',strftime('%Y-%m-%dT%H:%M:%fZ','now'),?7) ON CONFLICT(project_id) DO UPDATE SET local_hash=excluded.local_hash,revision=excluded.revision,message='Synced',synced_at=excluded.synced_at",params![incoming.project_id,destination.0,destination.1,destination.2,incoming.fingerprint()?,revision,!present]).map_err(storage)?;
        tx.commit().map_err(storage)
    }
}
