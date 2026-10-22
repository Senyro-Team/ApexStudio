use super::{storage, Workspace};
use apex_core::{Asset, Error, ImportReport, Project, ProjectInput};
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

impl Workspace {
    pub fn projects(&self) -> Result<Vec<Project>, Error> {
        let connection = self.connection()?;
        let mut query=connection.prepare("SELECT p.id,p.metadata,p.created_at,p.updated_at,p.archived,(SELECT count(*) FROM assets a WHERE a.project_id=p.id) FROM projects p ORDER BY p.updated_at DESC,p.id DESC").map_err(storage)?;
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
        self.projects()?
            .into_iter()
            .find(|project| project.id == id)
            .ok_or(Error::NotFound)
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
    pub fn assets(&self, project_id: Option<&str>) -> Result<Vec<Asset>, Error> {
        let connection = self.connection()?;
        let mut query=connection.prepare("SELECT a.id,a.project_id,a.name,a.hash,m.size,a.imported_at FROM assets a JOIN managed_files m ON a.hash=m.hash WHERE (?1 IS NULL OR a.project_id=?1) ORDER BY a.imported_at DESC,a.id DESC").map_err(storage)?;
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
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let path = self.root.join("thumbnails").join(format!("{hash}.png"));
        if fs::metadata(&path).ok()?.len() > 512 * 1024 {
            return None;
        }
        fs::read(path).ok().map(|bytes| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        })
    }
    pub(super) fn prepare_thumbnail(&self, hash: &str) {
        // Decode only bounded PNGs. Unsupported/malformed files remain ordinary assets.
        let result = (|| -> Option<()> {
            let path = self.root.join("files").join(hash);
            if fs::metadata(&path).ok()?.len() > 8 * 1024 * 1024 {
                return None;
            }
            let mut header = [0u8; 24];
            fs::File::open(&path).ok()?.read_exact(&mut header).ok()?;
            if &header[..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
                return None;
            }
            let width = u32::from_be_bytes(header[16..20].try_into().ok()?);
            let height = u32::from_be_bytes(header[20..24].try_into().ok()?);
            if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 16_000_000 {
                return None;
            }
            let image = image::load_from_memory_with_format(
                &fs::read(&path).ok()?,
                image::ImageFormat::Png,
            )
            .ok()?;
            let mut bytes = std::io::Cursor::new(Vec::new());
            image
                .thumbnail(320, 200)
                .write_to(&mut bytes, image::ImageFormat::Png)
                .ok()?;
            let target = self.root.join("thumbnails").join(format!("{hash}.png"));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)
                .ok()?;
            file.write_all(bytes.get_ref()).ok()?;
            Some(())
        })();
        let _ = result;
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
