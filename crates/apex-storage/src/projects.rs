use super::{storage, Workspace};
use apex_core::{Asset, Error, ImportReport, Project, ProjectInput};
use rusqlite::{params, OptionalExtension};
use std::{fs, path::Path};

impl Workspace {
    pub fn projects(&self) -> Result<Vec<Project>, Error> {
        let connection = self.connection()?;
        let mut query=connection.prepare("SELECT p.id,p.metadata,p.created_at,p.updated_at,p.archived,(SELECT count(*) FROM assets a WHERE a.project_id=p.id AND a.trash_id IS NULL) FROM projects p ORDER BY p.updated_at DESC,p.id DESC").map_err(storage)?;
        let rows = query
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .map_err(storage)?;
        rows.map(|row| {
            let (id, metadata, created_at, updated_at, archived, asset_count) =
                row.map_err(storage)?;
            Ok(Project {
                id,
                fields: serde_json::from_str(&metadata).map_err(storage)?,
                created_at,
                updated_at,
                archived,
                asset_count,
            })
        })
        .collect()
    }
    pub fn project(&self, id: &str) -> Result<Project, Error> {
        let db = self.connection()?;
        db.query_row("SELECT metadata,created_at,updated_at,archived,(SELECT count(*) FROM assets WHERE project_id=?1 AND trash_id IS NULL) FROM projects WHERE id=?1",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?,r.get::<_,i64>(4)?))).optional().map_err(storage)?.map(|(metadata,created_at,updated_at,archived,count)|Ok(Project{id:id.into(),fields:serde_json::from_str(&metadata).map_err(storage)?,created_at,updated_at,archived,asset_count:u32::try_from(count).map_err(storage)?})).unwrap_or(Err(Error::NotFound))
    }
    pub fn save_project(&self, id: Option<&str>, fields: ProjectInput) -> Result<Project, Error> {
        let fields = apex_core::validate_project(fields)?;
        let metadata = serde_json::to_string(&fields).map_err(storage)?;
        let connection = self.connection()?;
        let id = match id {
            Some(id) => {
                let existing = self.project(id)?;
                if existing.archived {
                    return Err(Error::Archived);
                }
                connection.execute("UPDATE projects SET metadata=?1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2 AND archived=0",params![metadata,id]).map_err(storage)?;
                id.to_owned()
            }
            None => {
                let id = uuid::Uuid::now_v7().to_string();
                connection
                    .execute(
                        "INSERT INTO projects(id,metadata) VALUES (?1,?2)",
                        params![id, metadata],
                    )
                    .map_err(storage)?;
                id
            }
        };
        self.project(&id)
    }
    pub fn archive_project(&self, id: &str, archived: bool) -> Result<Project, Error> {
        if self.connection()?.execute("UPDATE projects SET archived=?1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",params![archived,id]).map_err(storage)?==0{return Err(Error::NotFound);}
        self.project(id)
    }
    pub fn delete_archived_project(&self, id: &str) -> Result<(), Error> {
        let gate = self.content_gate();
        let _guard = gate.lock();
        let mut db = self.connection()?;
        let archived: bool = db
            .query_row("SELECT archived FROM projects WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        if !archived {
            return Err(Error::InvalidContent(
                "Archive the project before deleting it permanently.".into(),
            ));
        }
        let active: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM upload_batch_items i JOIN upload_batches b ON b.id=i.batch_id JOIN asset_folders f ON f.id=i.folder_id WHERE f.project_id=?1 AND b.state IN ('running','paused'))", [id], |row| row.get(0)).map_err(storage)?;
        if active {
            return Err(Error::InvalidContent(
                "Cancel active uploads before deleting this project.".into(),
            ));
        }
        let mut query = db.prepare("SELECT hash FROM assets WHERE project_id=?1 UNION SELECT v.hash FROM asset_versions v JOIN assets a ON a.id=v.asset_id WHERE a.project_id=?1 UNION SELECT cover_hash FROM asset_folders WHERE project_id=?1 AND cover_hash IS NOT NULL UNION SELECT i.hash FROM note_images i JOIN notes n ON n.id=i.note_id WHERE n.project_id=?1 UNION SELECT hash FROM publishing_media WHERE project_id=?1").map_err(storage)?;
        let hashes = query
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        drop(query);
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        tx.execute_batch("PRAGMA defer_foreign_keys=ON")
            .map_err(storage)?;
        tx.execute(
            "UPDATE project_sync SET enabled=0,message='Local project deleted' WHERE project_id=?1",
            [id],
        )
        .map_err(storage)?;
        tx.execute("DELETE FROM upload_batch_items WHERE folder_id IN (SELECT id FROM asset_folders WHERE project_id=?1)", [id]).map_err(storage)?;
        tx.execute("DELETE FROM asset_versions WHERE asset_id IN (SELECT id FROM assets WHERE project_id=?1)", [id]).map_err(storage)?;
        tx.execute("DELETE FROM asset_placements WHERE asset_id IN (SELECT id FROM assets WHERE project_id=?1)", [id]).map_err(storage)?;
        tx.execute("DELETE FROM assets WHERE project_id=?1", [id])
            .map_err(storage)?;
        tx.execute("DELETE FROM asset_folders WHERE project_id=?1", [id])
            .map_err(storage)?;
        tx.execute("DELETE FROM trash_entries WHERE project_id=?1", [id])
            .map_err(storage)?;
        tx.execute(
            "DELETE FROM note_images WHERE note_id IN (SELECT id FROM notes WHERE project_id=?1)",
            [id],
        )
        .map_err(storage)?;
        tx.execute("DELETE FROM notes WHERE project_id=?1", [id])
            .map_err(storage)?;
        tx.execute("DELETE FROM publishing_media WHERE project_id=?1", [id])
            .map_err(storage)?;
        tx.execute("DELETE FROM publishing_drafts WHERE project_id=?1", [id])
            .map_err(storage)?;
        tx.execute(
            "UPDATE posts SET legacy_project_id=NULL WHERE legacy_project_id=?1",
            [id],
        )
        .map_err(storage)?;
        let changed = tx
            .execute("DELETE FROM projects WHERE id=?1 AND archived=1", [id])
            .map_err(storage)?;
        if changed == 0 {
            return Err(Error::Conflict);
        }
        tx.commit().map_err(storage)?;
        self.collect_content(&hashes)
    }
    pub fn assets(&self, project_id: Option<&str>) -> Result<Vec<Asset>, Error> {
        let connection = self.connection()?;
        let mut query=connection.prepare("SELECT a.id,a.project_id,a.name,a.hash,m.size,a.imported_at FROM assets a JOIN managed_files m ON a.hash=m.hash WHERE a.trash_id IS NULL AND (?1 IS NULL OR a.project_id=?1) ORDER BY a.imported_at DESC,a.id DESC").map_err(storage)?;
        let rows = query
            .query_map([project_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get(5)?,
                ))
            })
            .map_err(storage)?;
        rows.map(|row| {
            let (id, project_id, name, hash, size, imported_at) = row.map_err(storage)?;
            Ok(Asset {
                id,
                project_id,
                name,
                preview: self.thumbnail(&hash),
                hash,
                size: u64::try_from(size).map_err(storage)?,
                imported_at,
            })
        })
        .collect()
    }
    pub(super) fn thumbnail(&self, hash: &str) -> Option<String> {
        apex_core::valid_hash(hash).then(|| format!("http://apex-thumb.localhost/{hash}"))
    }
    pub fn import_project_files(
        &self,
        project_id: &str,
        paths: &[std::path::PathBuf],
    ) -> Result<ImportReport, Error> {
        self.import_files_into(project_id, None, paths)
    }
    pub fn save_asset_copy(&self, asset_id: &str, destination: &Path) -> Result<(), Error> {
        let hash: Option<String> = self
            .connection()?
            .query_row("SELECT hash FROM assets WHERE id=?1", [asset_id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(storage)?;
        let hash = hash.ok_or(Error::NotFound)?;
        let mut input = fs::File::open(self.root.join("files").join(hash)).map_err(storage)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(storage)?;
        // Never overwrite existing user files; remove only a newly created incomplete copy on failure.
        let result = std::io::copy(&mut input, &mut output).and_then(|_| output.sync_all());
        drop(output);
        if let Err(error) = result {
            let _ = fs::remove_file(destination);
            return Err(storage(error));
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn permanent_delete_requires_archive_and_removes_project_content() {
        let temp = tempfile::tempdir().unwrap();
        let ws = Workspace::open(temp.path().join("workspace")).unwrap();
        let project = ws
            .save_project(
                None,
                ProjectInput {
                    name: "Old".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let other = ws
            .save_project(
                None,
                ProjectInput {
                    name: "Keep".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let folder = ws.save_folder(&project.id, None, None, "Parent").unwrap();
        ws.save_folder(&project.id, Some(&folder.id), None, "Child")
            .unwrap();
        let source = temp.path().join("image.png");
        image::RgbImage::new(2, 2).save(&source).unwrap();
        let asset = ws
            .import_project_files(&project.id, std::slice::from_ref(&source))
            .unwrap()
            .imported
            .remove(0);
        let note = ws.create_note(Some(&project.id)).unwrap();
        ws.attach_note_image(&note.id, &source).unwrap();
        assert!(ws.delete_archived_project(&project.id).is_err());
        ws.archive_project(&project.id, true).unwrap();
        ws.delete_archived_project(&project.id).unwrap();
        assert!(ws.project(&project.id).is_err());
        assert!(ws.note(&note.id).is_err());
        assert!(ws.folder(&folder.id).is_err());
        assert!(ws.project(&other.id).is_ok());
        assert!(source.exists());
        assert!(!ws.root.join("files").join(asset.hash).exists());
        let problems: i64 = ws
            .connection()
            .unwrap()
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(problems, 0);
    }
    #[test]
    fn project_lifecycle_preserves_metadata_and_originals_across_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("workspace");
        let workspace = Workspace::open(&root).unwrap();
        let fields = ProjectInput {
            name: "Test project".into(),
            tags: vec!["Police".into()],
            deadline: Some("2026-12-01".into()),
            ..Default::default()
        };
        let project = workspace.save_project(None, fields).unwrap();
        let original = temp.path().join("source.psd");
        fs::write(&original, b"sample source").unwrap();
        let missing = temp.path().join("missing.png");
        let report = workspace
            .import_project_files(&project.id, &[original.clone(), missing])
            .unwrap();
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.imported.len(), 1);
        assert_eq!(
            workspace
                .import_project_files(&project.id, std::slice::from_ref(&original))
                .unwrap()
                .skipped
                .len(),
            1
        );
        let workspace = Workspace::open(root).unwrap();
        let reopened = workspace.project(&project.id).unwrap();
        assert_eq!(reopened.fields.deadline, Some("2026-12-01".into()));
        assert_eq!(reopened.asset_count, 1);
        assert_eq!(fs::read(&original).unwrap(), b"sample source");
        workspace.archive_project(&project.id, true).unwrap();
        assert!(matches!(
            workspace.import_project_files(&project.id, &[original]),
            Err(Error::Archived)
        ));
        workspace.archive_project(&project.id, false).unwrap();
        let copy = temp.path().join("copy.psd");
        workspace
            .save_asset_copy(&report.imported[0].id, &copy)
            .unwrap();
        assert_eq!(fs::read(&copy).unwrap(), b"sample source");
        assert!(workspace
            .save_asset_copy(&report.imported[0].id, &copy)
            .is_err());
    }
    #[test]
    fn version_one_migration_keeps_existing_workspace_name() {
        let temp = tempfile::tempdir().unwrap();
        let db = rusqlite::Connection::open(temp.path().join("workspace.sqlite3")).unwrap();
        db.execute_batch("CREATE TABLE workspace_settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);INSERT INTO workspace_settings VALUES('name','Existing studio');CREATE TABLE managed_files(hash TEXT PRIMARY KEY,size INTEGER NOT NULL,imported_at TEXT NOT NULL);PRAGMA user_version=1;").unwrap();
        drop(db);
        let workspace = Workspace::open(temp.path()).unwrap();
        use apex_core::WorkspaceRepository;
        assert_eq!(workspace.info().unwrap().name, "Existing studio");
        assert!(workspace.projects().unwrap().is_empty());
    }
}
