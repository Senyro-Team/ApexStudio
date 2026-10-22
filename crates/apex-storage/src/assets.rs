use super::{storage, Workspace};
use apex_core::{AssetBoard, AssetFolder, AssetVersion, Error, ImportReport, Placement};
use rusqlite::{params, OptionalExtension};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
fn bad(s: &str) -> Error {
    Error::InvalidContent(s.into())
}
pub fn validate_folder_name(name: &str) -> Result<(), Error> {
    if name.trim().is_empty()
        || name.chars().count() > 120
        || name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(bad(
            "Enter a folder name up to 120 characters without slashes.",
        ));
    }
    Ok(())
}
impl Workspace {
    pub fn editable_project(&self, id: &str) -> Result<(), Error> {
        if self.project(id)?.archived {
            return Err(Error::Archived);
        }
        Ok(())
    }
    pub fn folders(&self, project: &str) -> Result<Vec<AssetFolder>, Error> {
        let db = self.connection()?;
        let mut s=db.prepare("SELECT id,project_id,parent_id,name,kind,vehicle_id,cover_hash FROM asset_folders WHERE project_id=?1 ORDER BY name COLLATE NOCASE").map_err(storage)?;
        let result = s
            .query_map([project], |r| {
                Ok(AssetFolder {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    parent_id: r.get(2)?,
                    name: r.get(3)?,
                    kind: r.get(4)?,
                    vehicle_id: r.get(5)?,
                    cover_hash: r.get(6)?,
                    preview: None,
                })
            })
            .map_err(storage)?
            .map(|r| {
                let mut f = r.map_err(storage)?;
                f.preview = f.cover_hash.as_deref().and_then(|h| self.thumbnail(h));
                Ok(f)
            })
            .collect();
        result
    }
    pub fn folder(&self, id: &str) -> Result<AssetFolder, Error> {
        let db = self.connection()?;
        let project: Option<String> = db
            .query_row(
                "SELECT project_id FROM asset_folders WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        self.folders(&project.ok_or(Error::NotFound)?)?
            .into_iter()
            .find(|f| f.id == id)
            .ok_or(Error::NotFound)
    }
    pub fn board(&self, project: &str) -> Result<AssetBoard, Error> {
        let db = self.connection()?;
        let mut s=db.prepare("SELECT p.asset_id,p.folder_id,p.slot FROM asset_placements p JOIN assets a ON a.id=p.asset_id WHERE a.project_id=?1").map_err(storage)?;
        let placements = s
            .query_map([project], |r| {
                Ok(Placement {
                    asset_id: r.get(0)?,
                    folder_id: r.get(1)?,
                    slot: r.get(2)?,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        Ok(AssetBoard {
            assets: self.assets(Some(project))?,
            folders: self.folders(project)?,
            placements,
        })
    }
    pub fn validate_parent(&self, project: &str, parent: Option<&str>) -> Result<(), Error> {
        if let Some(id) = parent {
            let p = self.folder(id)?;
            if p.project_id != project || p.kind != "folder" {
                return Err(bad("Choose a regular folder in this project."));
            }
        }
        Ok(())
    }
    pub fn save_folder(
        &self,
        project: &str,
        id: Option<&str>,
        parent: Option<&str>,
        name: &str,
    ) -> Result<AssetFolder, Error> {
        self.editable_project(project)?;
        validate_folder_name(name)?;
        self.validate_parent(project, parent)?;
        let id = match id {
            Some(id) => {
                let f = self.folder(id)?;
                if f.project_id != project {
                    return Err(Error::NotFound);
                }
                let mut cursor = parent.map(str::to_owned);
                let mut visited = HashSet::new();
                while let Some(next) = cursor {
                    if next == id || !visited.insert(next.clone()) {
                        return Err(bad("A folder cannot be moved inside itself."));
                    }
                    cursor = self.folder(&next)?.parent_id;
                }
                self.connection()?
                    .execute(
                        "UPDATE asset_folders SET name=?1,parent_id=?2 WHERE id=?3",
                        params![name.trim(), parent, id],
                    )
                    .map_err(storage)?;
                id.to_owned()
            }
            None => {
                let id = uuid::Uuid::now_v7().to_string();
                self.connection()?.execute("INSERT INTO asset_folders(id,project_id,parent_id,name,kind)VALUES(?1,?2,?3,?4,'folder')",params![id,project,parent,name.trim()]).map_err(storage)?;
                id
            }
        };
        self.folder(&id)
    }
    pub fn move_asset(&self, id: &str, folder: Option<&str>) -> Result<(), Error> {
        let asset = self
            .assets(None)?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or(Error::NotFound)?;
        self.editable_project(&asset.project_id)?;
        if let Some(fid) = folder {
            if self.folder(fid)?.project_id != asset.project_id {
                return Err(bad("Choose a folder in this project."));
            }
        }
        self.connection()?.execute("INSERT INTO asset_placements(asset_id,folder_id,slot) VALUES(?1,?2,NULL) ON CONFLICT(asset_id) DO UPDATE SET folder_id=excluded.folder_id,slot=NULL",params![id,folder]).map_err(storage)?;
        Ok(())
    }
    pub fn import_files_into(
        &self,
        project: &str,
        folder: Option<&str>,
        paths: &[PathBuf],
    ) -> Result<ImportReport, Error> {
        self.editable_project(project)?;
        if let Some(id) = folder {
            if self.folder(id)?.project_id != project {
                return Err(Error::NotFound);
            }
        }
        let mut report = ImportReport::default();
        for path in paths {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let result = (|| -> Result<bool, Error> {
                let hash = self.import_file(path)?;
                let mut db = self.connection()?;
                let tx = db.transaction().map_err(storage)?;
                let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM assets a LEFT JOIN asset_placements p ON p.asset_id=a.id WHERE a.project_id=?1 AND a.name=?2 AND a.hash=?3 AND p.folder_id IS ?4)",params![project,name,hash,folder],|r|r.get(0)).map_err(storage)?;
                if exists {
                    return Ok(false);
                }
                let archived: bool = tx
                    .query_row(
                        "SELECT archived FROM projects WHERE id=?1",
                        [project],
                        |r| r.get(0),
                    )
                    .map_err(storage)?;
                if archived {
                    return Err(Error::Archived);
                }
                let id = uuid::Uuid::now_v7().to_string();
                tx.execute(
                    "INSERT INTO assets(id,project_id,name,hash) VALUES(?1,?2,?3,?4)",
                    params![id, project, name, hash],
                )
                .map_err(storage)?;
                tx.execute(
                    "INSERT INTO asset_placements(asset_id,folder_id) VALUES(?1,?2)",
                    params![id, folder],
                )
                .map_err(storage)?;
                tx.execute("UPDATE projects SET updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",[project]).map_err(storage)?;
                tx.commit().map_err(storage)?;
                self.prepare_thumbnail(&hash);
                Ok(true)
            })();
            match result {
                Ok(true) => (),
                Ok(false) => report.skipped.push(name),
                Err(e) => report.failed.push(format!("{name}: {e}")),
            }
        }
        report.imported = self.assets(Some(project))?;
        Ok(report)
    }
    pub fn set_cover(&self, id: &str, path: &Path) -> Result<(), Error> {
        let f = self.folder(id)?;
        self.editable_project(&f.project_id)?;
        validate_png(path)?;
        let hash = self.import_file(path)?;
        self.prepare_thumbnail(&hash);
        self.connection()?
            .execute(
                "UPDATE asset_folders SET cover_hash=?1 WHERE id=?2",
                params![hash, id],
            )
            .map_err(storage)?;
        Ok(())
    }
    pub fn versions(&self, id: &str) -> Result<Vec<AssetVersion>, Error> {
        let db = self.connection()?;
        let mut s=db.prepare("SELECT id,asset_id,hash,name,created_at FROM asset_versions WHERE asset_id=?1 ORDER BY created_at DESC,id DESC").map_err(storage)?;
        let result = s
            .query_map([id], |r| {
                Ok(AssetVersion {
                    id: r.get(0)?,
                    asset_id: r.get(1)?,
                    hash: r.get(2)?,
                    name: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage);
        result
    }
    pub fn restore_version(&self, id: &str, version: &str) -> Result<(), Error> {
        let asset = self
            .assets(None)?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or(Error::NotFound)?;
        self.editable_project(&asset.project_id)?;
        let v = self
            .versions(id)?
            .into_iter()
            .find(|v| v.id == version)
            .ok_or(Error::NotFound)?;
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(storage)?;
        tx.execute(
            "INSERT OR IGNORE INTO asset_versions(id,asset_id,hash,name) VALUES(?1,?2,?3,?4)",
            params![uuid::Uuid::now_v7().to_string(), id, asset.hash, asset.name],
        )
        .map_err(storage)?;
        tx.execute(
            "UPDATE assets SET hash=?1,name=?2 WHERE id=?3",
            params![v.hash, v.name, id],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Reviewed files and target metadata are committed in one atomic livery import."
    )]
    pub fn import_livery(
        &self,
        project: &str,
        parent: Option<&str>,
        existing: Option<&str>,
        name: &str,
        vehicle: &str,
        files: &[(PathBuf, String, Option<String>)],
        cover: Option<usize>,
    ) -> Result<String, Error> {
        self.editable_project(project)?;
        validate_folder_name(name)?;
        self.validate_parent(project, parent)?;
        let catalogue = apex_core::vehicle_catalogue()?;
        let v = catalogue
            .vehicles
            .iter()
            .find(|v| v.id == vehicle)
            .ok_or_else(|| bad("Choose a vehicle from the catalogue."))?;
        if files.is_empty() || files.len() > 100 {
            return Err(bad("Choose between 1 and 100 PNG textures."));
        }
        if cover.is_some_and(|c| c >= files.len()) {
            return Err(bad("Invalid cover selection."));
        }
        let mut seen = HashSet::new();
        let mut prepared = Vec::new();
        for (path, filename, slot) in files {
            validate_png(path)?;
            if let Some(slot) = slot {
                if !v.slots.iter().any(|s| s.key == *slot) || !seen.insert(slot.clone()) {
                    return Err(bad(
                        "Resolve duplicate or unsupported slots before importing.",
                    ));
                }
            }
            let hash = self.import_file(path)?;
            self.prepare_thumbnail(&hash);
            prepared.push((filename, hash, slot));
        }
        let id = if let Some(id) = existing {
            let f = self.folder(id)?;
            if f.project_id != project
                || f.kind != "livery"
                || f.vehicle_id.as_deref() != Some(vehicle)
            {
                return Err(bad("Choose the matching existing livery."));
            }
            id.to_owned()
        } else {
            uuid::Uuid::now_v7().to_string()
        };
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(storage)?;
        let archived: bool = tx
            .query_row(
                "SELECT archived FROM projects WHERE id=?1",
                [project],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if archived {
            return Err(Error::Archived);
        }
        if existing.is_none() {
            tx.execute("INSERT INTO asset_folders(id,project_id,parent_id,name,kind,vehicle_id)VALUES(?1,?2,?3,?4,'livery',?5)",params![id,project,parent,name.trim(),vehicle]).map_err(storage)?;
        }
        if let Some(index) = cover {
            tx.execute(
                "UPDATE asset_folders SET cover_hash=?1 WHERE id=?2",
                params![prepared[index].1, id],
            )
            .map_err(storage)?;
        }
        for (filename, hash, slot) in prepared {
            let old: Option<(String, String, String)> = if let Some(slot) = slot {
                tx.query_row("SELECT a.id,a.hash,a.name FROM assets a JOIN asset_placements p ON p.asset_id=a.id WHERE p.folder_id=?1 AND p.slot=?2",params![id,slot],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage)?
            } else {
                None
            };
            let aid = if let Some((aid, oldhash, oldname)) = old {
                tx.execute("INSERT OR IGNORE INTO asset_versions(id,asset_id,hash,name) VALUES(?1,?2,?3,?4)",params![uuid::Uuid::now_v7().to_string(),aid,oldhash,oldname]).map_err(storage)?;
                tx.execute("UPDATE assets SET hash=?1,name=?2,imported_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?3",params![hash,filename,aid]).map_err(storage)?;
                aid
            } else {
                let aid = uuid::Uuid::now_v7().to_string();
                tx.execute(
                    "INSERT INTO assets(id,project_id,name,hash)VALUES(?1,?2,?3,?4)",
                    params![aid, project, filename, hash],
                )
                .map_err(storage)?;
                tx.execute(
                    "INSERT INTO asset_placements(asset_id,folder_id,slot)VALUES(?1,?2,?3)",
                    params![aid, id, slot],
                )
                .map_err(storage)?;
                aid
            };
            tx.execute(
                "INSERT OR IGNORE INTO asset_versions(id,asset_id,hash,name) VALUES(?1,?2,?3,?4)",
                params![uuid::Uuid::now_v7().to_string(), aid, hash, filename],
            )
            .map_err(storage)?;
        }
        tx.execute(
            "UPDATE projects SET updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
            [project],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(id)
    }
}
pub fn validate_png(path: &Path) -> Result<(), Error> {
    if fs::metadata(path).map_err(storage)?.len() > 20 * 1024 * 1024 {
        return Err(bad("Choose PNG textures smaller than 20 MB."));
    }
    let mut r = image::ImageReader::with_format(
        std::io::BufReader::new(fs::File::open(path).map_err(storage)?),
        image::ImageFormat::Png,
    );
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8000);
    limits.max_image_height = Some(8000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    r.limits(limits);
    r.decode()
        .map_err(|_| bad("Choose a valid PNG texture up to 8000 × 8000 pixels."))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use apex_core::{ProjectInput, WorkspaceRepository};
    fn png(path: &Path, color: u8) {
        image::RgbImage::from_pixel(2, 2, image::Rgb([color, 0, 0]))
            .save(path)
            .unwrap();
    }
    #[test]
    fn folders_liveries_versions_and_packages_survive_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                ProjectInput {
                    name: "Project".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let other = ws
            .save_project(
                None,
                ProjectInput {
                    name: "Other".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let root = ws.save_folder(&p.id, None, None, "Liveries").unwrap();
        let child = ws
            .save_folder(&p.id, None, Some(&root.id), "Sources")
            .unwrap();
        assert!(ws
            .save_folder(&p.id, Some(&root.id), Some(&child.id), "Liveries")
            .is_err());
        assert!(ws
            .save_folder(&other.id, None, Some(&root.id), "Wrong")
            .is_err());
        let catalogue = apex_core::vehicle_catalogue().unwrap();
        let vehicle = &catalogue.vehicles[0];
        let path = dir.path().join("Patrol Lft1_7.png");
        png(&path, 1);
        let files = vec![(
            path.clone(),
            "Patrol Lft1_7.png".into(),
            Some("Left 1".into()),
        )];
        let a = ws
            .import_livery(
                &p.id,
                Some(&root.id),
                None,
                "Patrol",
                &vehicle.id,
                &files,
                Some(0),
            )
            .unwrap();
        let b = ws
            .import_livery(
                &p.id,
                Some(&root.id),
                None,
                "Sheriff",
                &vehicle.id,
                &files,
                None,
            )
            .unwrap();
        assert_ne!(a, b);
        assert_eq!(ws.assets(Some(&p.id)).unwrap().len(), 2);
        let board = ws.board(&p.id).unwrap();
        let aid = board
            .placements
            .iter()
            .find(|p| p.folder_id.as_deref() == Some(&a))
            .unwrap()
            .asset_id
            .clone();
        let first = ws
            .assets(Some(&p.id))
            .unwrap()
            .into_iter()
            .find(|x| x.id == aid)
            .unwrap()
            .hash;
        png(&path, 2);
        ws.import_livery(
            &p.id,
            Some(&root.id),
            Some(&a),
            "Patrol",
            &vehicle.id,
            &files,
            None,
        )
        .unwrap();
        assert_eq!(ws.versions(&aid).unwrap().len(), 2);
        let old = ws
            .versions(&aid)
            .unwrap()
            .into_iter()
            .find(|v| v.hash == first)
            .unwrap();
        ws.restore_version(&aid, &old.id).unwrap();
        assert_eq!(
            ws.assets(Some(&p.id))
                .unwrap()
                .into_iter()
                .find(|x| x.id == aid)
                .unwrap()
                .hash,
            first
        );
        let reopened = Workspace::open(dir.path().join("ws")).unwrap();
        assert_eq!(reopened.folders(&p.id).unwrap().len(), 4);
        assert!(ws
            .import_livery(&p.id, Some(&a), None, "Nested", &vehicle.id, &files, None)
            .is_err());
        let package = dir.path().join("project.apexstd");
        ws.export_project(&p.id, true, true, &package).unwrap();
        let imported = Workspace::open(dir.path().join("imported")).unwrap();
        let pid = imported.import_package(&package).unwrap();
        let state = imported.board(&pid).unwrap();
        assert_eq!(state.folders.len(), 4);
        assert_eq!(state.placements.len(), 2);
        assert!(state
            .folders
            .iter()
            .any(|f| f.cover_hash.as_deref() == Some(&first)));
        assert!(state
            .assets
            .iter()
            .any(|a| imported.versions(&a.id).unwrap().len() == 2));
        let backup = dir.path().join("backup.apexbackup");
        ws.backup(&backup).unwrap();
        let restored = Workspace::open(dir.path().join("restored")).unwrap();
        restored.restore_backup(&backup).unwrap();
        assert_eq!(restored.board(&p.id).unwrap().folders.len(), 4);
        assert_eq!(restored.versions(&aid).unwrap().len(), 2);
    }
    #[test]
    fn invalid_livery_batch_never_creates_partial_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                ProjectInput {
                    name: "Project".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let path = dir.path().join("left.png");
        png(&path, 1);
        let vehicle = apex_core::vehicle_catalogue().unwrap().vehicles.remove(0);
        let files = vec![
            (path.clone(), "Left1.png".into(), Some("Left 1".into())),
            (path, "Left1again.png".into(), Some("Left 1".into())),
        ];
        assert!(ws
            .import_livery(&p.id, None, None, "Bad", &vehicle.id, &files, None)
            .is_err());
        assert!(ws.folders(&p.id).unwrap().is_empty());
        assert!(ws.assets(Some(&p.id)).unwrap().is_empty());
    }
    #[test]
    fn schema_three_backup_restores_and_migrates_with_safety_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("old")).unwrap();
        ws.rename("Old studio").unwrap();
        ws.connection().unwrap().execute_batch("DROP TABLE asset_versions;DROP TABLE asset_placements;DROP TABLE asset_folders;PRAGMA user_version=3;").unwrap();
        let path = dir.path().join("old.apexbackup");
        ws.backup(&path).unwrap();
        let restored = Workspace::open(dir.path().join("new")).unwrap();
        restored.restore_backup(&path).unwrap();
        assert_eq!(restored.info().unwrap().schema_version, 4);
        assert_eq!(restored.info().unwrap().name, "Old studio");
        let migrated = Workspace::open(dir.path().join("old")).unwrap();
        assert_eq!(migrated.info().unwrap().schema_version, 4);
        assert!(std::fs::read_dir(migrated.root.join("backups"))
            .unwrap()
            .next()
            .is_some());
    }
}
