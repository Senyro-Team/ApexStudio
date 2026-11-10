use super::{storage, Workspace};
use apex_core::{AssetBoard, AssetFolder, AssetVersion, Error, ImportReport, Placement};
use image::GenericImageView;
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
        let archived = self
            .connection()?
            .query_row("SELECT archived FROM projects WHERE id=?1", [id], |r| {
                r.get::<_, bool>(0)
            })
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        if archived {
            Err(Error::Archived)
        } else {
            Ok(())
        }
    }
    pub fn folders(&self, project: &str) -> Result<Vec<AssetFolder>, Error> {
        let db = self.connection()?;
        let mut s=db.prepare("SELECT id,project_id,parent_id,name,kind,vehicle_id,cover_hash FROM asset_folders WHERE project_id=?1 AND trash_id IS NULL ORDER BY name COLLATE NOCASE").map_err(storage)?;
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
                f.preview = f
                    .cover_hash
                    .as_deref()
                    .and_then(|h| self.thumbnail_for_project(h, project));
                Ok(f)
            })
            .collect();
        result
    }
    pub fn folder(&self, id: &str) -> Result<AssetFolder, Error> {
        let mut f=self.connection()?.query_row("SELECT id,project_id,parent_id,name,kind,vehicle_id,cover_hash FROM asset_folders WHERE id=?1 AND trash_id IS NULL",[id],|r|Ok(AssetFolder{id:r.get(0)?,project_id:r.get(1)?,parent_id:r.get(2)?,name:r.get(3)?,kind:r.get(4)?,vehicle_id:r.get(5)?,cover_hash:r.get(6)?,preview:None})).optional().map_err(storage)?.ok_or(Error::NotFound)?;
        f.preview = f
            .cover_hash
            .as_deref()
            .and_then(|h| self.thumbnail_for_project(h, &f.project_id));
        Ok(f)
    }
    pub fn board(&self, project: &str) -> Result<AssetBoard, Error> {
        let db = self.connection()?;
        let mut s=db.prepare("SELECT p.asset_id,p.folder_id,p.slot FROM asset_placements p JOIN assets a ON a.id=p.asset_id WHERE a.project_id=?1 AND a.trash_id IS NULL").map_err(storage)?;
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
        let collision: bool = self.connection()?.query_row("SELECT EXISTS(SELECT 1 FROM asset_folders WHERE project_id=?1 AND trash_id IS NULL AND parent_id IS ?2 AND name=?3 COLLATE NOCASE AND (?4 IS NULL OR id<>?4))",params![project,parent,name.trim(),id],|r|r.get(0)).map_err(storage)?;
        if collision {
            return Err(bad(
                "A folder with this name already exists here. Rename it before moving.",
            ));
        }
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
        let current: Option<Option<String>> = self
            .connection()?
            .query_row(
                "SELECT folder_id FROM asset_placements WHERE asset_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        if current.flatten().as_deref() == folder {
            return Ok(());
        }
        self.connection()?.execute("INSERT INTO asset_placements(asset_id,folder_id,slot) VALUES(?1,?2,NULL) ON CONFLICT(asset_id) DO UPDATE SET folder_id=excluded.folder_id,slot=NULL",params![id,folder]).map_err(storage)?;
        Ok(())
    }
    pub fn assign_slot(&self, id: &str, slot: Option<&str>) -> Result<(), Error> {
        let a = self
            .assets(None)?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or(Error::NotFound)?;
        self.editable_project(&a.project_id)?;
        let board = self.board(&a.project_id)?;
        let placement = board
            .placements
            .iter()
            .find(|p| p.asset_id == id)
            .ok_or(Error::NotFound)?;
        let folder = self.folder(
            placement
                .folder_id
                .as_deref()
                .ok_or_else(|| bad("Open a livery to assign a side."))?,
        )?;
        if folder.kind != "livery" || slot.is_some_and(|s| !valid_slot(s)) {
            return Err(bad("Choose a side and a number from 1 to 100."));
        }
        if slot.is_some()
            && board.placements.iter().any(|p| {
                p.asset_id != id && p.folder_id == placement.folder_id && p.slot.as_deref() == slot
            })
        {
            return Err(bad(
                "That slot is already assigned. Clear its existing assignment first.",
            ));
        }
        self.connection()?
            .execute(
                "UPDATE asset_placements SET slot=?1 WHERE asset_id=?2",
                params![slot, id],
            )
            .map_err(storage)?;
        Ok(())
    }

    /// Copy a tree in one transaction. Immutable blobs are shared; records and history get new IDs.
    pub fn copy_item(
        &self,
        project: &str,
        id: &str,
        is_folder: bool,
        target: Option<&str>,
    ) -> Result<(), Error> {
        self.editable_project(project)?;
        let board = self.board(project)?;
        if is_folder {
            self.validate_parent(project, target)?;
        } else if let Some(fid) = target {
            if self.folder(fid)?.project_id != project {
                return Err(Error::NotFound);
            }
        }
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(storage)?;
        let mut mapping = std::collections::HashMap::new();
        if is_folder {
            let root = board
                .folders
                .iter()
                .find(|f| f.id == id)
                .ok_or(Error::NotFound)?;
            let mut children = vec![root];
            let mut i = 0;
            while i < children.len() {
                let current = children[i].id.clone();
                if children.len() > 2000 {
                    return Err(bad("This folder contains too many folders to copy."));
                }
                children.extend(
                    board
                        .folders
                        .iter()
                        .filter(|f| f.parent_id.as_deref() == Some(&current)),
                );
                i += 1;
            }
            if target.is_some_and(|t| children.iter().any(|f| f.id == t)) {
                return Err(bad("A folder cannot be copied inside itself."));
            }
            let mut name = root.name.clone();
            let mut number = 1;
            while board
                .folders
                .iter()
                .any(|f| f.parent_id.as_deref() == target && f.name.eq_ignore_ascii_case(&name))
            {
                name = format!(
                    "{} (copy {})",
                    root.name.chars().take(100).collect::<String>(),
                    number
                );
                number += 1;
            }
            for f in children {
                let new_id = uuid::Uuid::now_v7().to_string();
                let parent = if f.id == id {
                    target.map(str::to_owned)
                } else {
                    f.parent_id.as_ref().and_then(|p| mapping.get(p).cloned())
                };
                tx.execute("INSERT INTO asset_folders(id,project_id,parent_id,name,kind,vehicle_id,cover_hash) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![new_id,project,parent,if f.id==id {&name} else {&f.name},f.kind,f.vehicle_id,f.cover_hash]).map_err(storage)?;
                mapping.insert(f.id.clone(), new_id);
            }
        } else if !board.assets.iter().any(|a| a.id == id) {
            return Err(Error::NotFound);
        }
        for a in &board.assets {
            let placement = board.placements.iter().find(|p| p.asset_id == a.id);
            let old_folder = placement.and_then(|p| p.folder_id.as_ref());
            if (is_folder && !old_folder.is_some_and(|f| mapping.contains_key(f)))
                || (!is_folder && a.id != id)
            {
                continue;
            }
            let destination = if is_folder {
                old_folder.and_then(|f| mapping.get(f)).map(String::as_str)
            } else {
                target
            };
            let slot = if is_folder {
                placement.and_then(|p| p.slot.as_deref())
            } else {
                None
            };
            let aid = uuid::Uuid::now_v7().to_string();
            let mut name = a.name.clone();
            if !is_folder {
                let mut number = 1;
                while board.assets.iter().any(|other| {
                    other.name.eq_ignore_ascii_case(&name)
                        && board
                            .placements
                            .iter()
                            .find(|p| p.asset_id == other.id)
                            .and_then(|p| p.folder_id.as_deref())
                            == target
                }) {
                    let path = Path::new(&a.name);
                    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                    name = match path.extension() {
                        Some(ext) => format!("{stem} (copy {number}).{}", ext.to_string_lossy()),
                        None => format!("{stem} (copy {number})"),
                    };
                    number += 1;
                }
            }
            tx.execute(
                "INSERT INTO assets(id,project_id,name,hash) VALUES(?1,?2,?3,?4)",
                params![aid, project, name, a.hash],
            )
            .map_err(storage)?;
            tx.execute(
                "INSERT INTO asset_placements(asset_id,folder_id,slot) VALUES(?1,?2,?3)",
                params![aid, destination, slot],
            )
            .map_err(storage)?;
            for v in self.versions(&a.id)? {
                tx.execute("INSERT INTO asset_versions(id,asset_id,hash,name,created_at) VALUES(?1,?2,?3,?4,?5)",params![uuid::Uuid::now_v7().to_string(),aid,v.hash,v.name,v.created_at]).map_err(storage)?;
            }
        }
        tx.commit().map_err(storage)?;
        Ok(())
    }
    pub fn import_files_into(
        &self,
        project: &str,
        folder: Option<&str>,
        paths: &[PathBuf],
    ) -> Result<ImportReport, Error> {
        let gate = self.content_gate();
        let _content = gate.lock();
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
                let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM assets a LEFT JOIN asset_placements p ON p.asset_id=a.id WHERE a.project_id=?1 AND a.trash_id IS NULL AND a.name=?2 AND a.hash=?3 AND p.folder_id IS ?4)",params![project,name,hash,folder],|r|r.get(0)).map_err(storage)?;
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
        let gate = self.content_gate();
        let _content = gate.lock();
        let f = self.folder(id)?;
        self.editable_project(&f.project_id)?;
        validate_png(path)?;
        let hash = self.import_file(path)?;

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
        let prepared = files
            .iter()
            .map(|(path, name, slot)| Ok((self.prepare_texture(path)?, name.clone(), slot.clone())))
            .collect::<Result<Vec<_>, Error>>()?;
        self.import_prepared_livery(project, parent, existing, name, vehicle, &prepared, cover)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "One atomic reviewed livery import."
    )]
    pub fn import_prepared_livery(
        &self,
        project: &str,
        parent: Option<&str>,
        existing: Option<&str>,
        name: &str,
        vehicle: &str,
        files: &[(crate::PreparedTexture, String, Option<String>)],
        cover: Option<usize>,
    ) -> Result<String, Error> {
        let gate = self.content_gate();
        let _content = gate.lock();
        self.editable_project(project)?;
        validate_folder_name(name)?;
        self.validate_parent(project, parent)?;
        if !apex_core::vehicle_catalogue()?
            .vehicles
            .iter()
            .any(|v| v.id == vehicle)
        {
            return Err(bad("Choose a vehicle from the catalogue."));
        }
        if files.is_empty() || files.len() > 100 || cover.is_some_and(|i| i >= files.len()) {
            return Err(bad("Choose 1–100 PNGs and a valid cover."));
        }
        let mut seen = HashSet::new();
        let mut prepared = Vec::new();
        for (texture, filename, slot) in files {
            if let Some(slot) = slot {
                if !valid_slot(slot) || !seen.insert(slot) {
                    return Err(bad("Resolve duplicate or invalid slots."));
                }
            }
            self.promote_texture(texture)?;
            prepared.push((filename, texture.hash(), slot));
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
        for (texture, _, _) in files {
            Self::register_texture(&tx, texture)?;
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
                tx.query_row("SELECT a.id,a.hash,a.name FROM assets a JOIN asset_placements p ON p.asset_id=a.id WHERE p.folder_id=?1 AND p.slot=?2 AND a.trash_id IS NULL",params![id,slot],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage)?
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
fn valid_slot(slot: &str) -> bool {
    let Some((side, number)) = slot.rsplit_once(' ') else {
        return false;
    };
    [
        "Left",
        "Right",
        "Front",
        "Frontplate",
        "Back",
        "Top",
        "Window Left",
        "Window Right",
        "Window Front",
        "Window Back",
        "Window Top",
    ]
    .contains(&side)
        && number
            .parse::<u8>()
            .is_ok_and(|n| (1..=100).contains(&n) && n.to_string() == number)
}
pub fn validate_png_dimensions(path: &Path) -> Result<(u32, u32), Error> {
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
    let image = r
        .decode()
        .map_err(|_| bad("Choose a valid PNG texture up to 8000 × 8000 pixels."))?;
    Ok(image.dimensions())
}
pub fn validate_png(path: &Path) -> Result<(), Error> {
    validate_png_dimensions(path).map(|_| ())
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
    fn explorer_copy_preserves_livery_history_and_rejects_invalid_targets() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                ProjectInput {
                    name: "Explorer".into(),
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
        let target = ws.save_folder(&p.id, None, None, "Destination").unwrap();
        let vehicle = apex_core::vehicle_catalogue().unwrap().vehicles.remove(0);
        let path = dir.path().join("Left1.png");
        png(&path, 1);
        let files = vec![(path.clone(), "Left1.png".into(), Some("Left 1".into()))];
        let livery = ws
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
        png(&path, 2);
        ws.import_livery(
            &p.id,
            Some(&root.id),
            Some(&livery),
            "Patrol",
            &vehicle.id,
            &files,
            None,
        )
        .unwrap();
        let aid = ws.board(&p.id).unwrap().assets[0].id.clone();
        ws.assign_slot(&aid, Some("Window Front 2")).unwrap();
        assert!(ws.assign_slot(&aid, Some("Window Front 0")).is_err());
        assert!(ws.copy_item(&p.id, &root.id, true, Some(&root.id)).is_err());
        assert!(ws.copy_item(&other.id, &root.id, true, None).is_err());
        assert!(ws.copy_item(&p.id, &root.id, true, Some(&livery)).is_err());
        ws.copy_item(&p.id, &root.id, true, Some(&target.id))
            .unwrap();
        let board = ws.board(&p.id).unwrap();
        assert_eq!(board.assets.len(), 2);
        let cloned = board.assets.iter().find(|a| a.id != aid).unwrap();
        assert_eq!(ws.versions(&cloned.id).unwrap().len(), 2);
        assert_eq!(
            board
                .placements
                .iter()
                .find(|p| p.asset_id == cloned.id)
                .unwrap()
                .slot
                .as_deref(),
            Some("Window Front 2")
        );
        let copied_root = board
            .folders
            .iter()
            .find(|f| f.parent_id.as_deref() == Some(&target.id))
            .unwrap();
        let copied_livery = board
            .folders
            .iter()
            .find(|f| f.parent_id.as_deref() == Some(&copied_root.id))
            .unwrap();
        assert_eq!(
            copied_livery.cover_hash,
            ws.folder(&livery).unwrap().cover_hash
        );
        ws.copy_item(&p.id, &aid, false, Some(&livery)).unwrap();
        let board = ws.board(&p.id).unwrap();
        let copied_file = board
            .assets
            .iter()
            .find(|a| a.id != aid && a.id != cloned.id)
            .unwrap();
        assert!(ws
            .assign_slot(&copied_file.id, Some("Window Front 2"))
            .is_err());
        ws.move_asset(&aid, Some(&livery)).unwrap();
        assert_eq!(
            ws.board(&p.id)
                .unwrap()
                .placements
                .iter()
                .find(|p| p.asset_id == aid)
                .unwrap()
                .slot
                .as_deref(),
            Some("Window Front 2")
        );
        ws.move_asset(&aid, Some(&target.id)).unwrap();
        assert!(ws
            .board(&p.id)
            .unwrap()
            .placements
            .iter()
            .find(|p| p.asset_id == aid)
            .unwrap()
            .slot
            .is_none());
        ws.save_folder(&p.id, Some(&livery), None, "Patrol")
            .unwrap();
        assert!(ws.folder(&livery).unwrap().parent_id.is_none());
    }
    #[test]
    fn large_png_previews_regenerate_after_cache_loss() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                ProjectInput {
                    name: "Previews".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let path = dir.path().join("large.png");
        image::RgbImage::from_pixel(4100, 4000, image::Rgb([50, 100, 150]))
            .save(&path)
            .unwrap();
        ws.import_files_into(&p.id, None, &[path]).unwrap();
        let asset = ws.assets(Some(&p.id)).unwrap().remove(0);
        assert!(asset.preview.is_some());
        assert!(!ws.thumbnail_bytes(&asset.hash).unwrap().is_empty());
        fs::remove_file(
            ws.root
                .join("thumbnails")
                .join(format!("{}.png", asset.hash)),
        )
        .unwrap();
        assert!(ws.assets(Some(&p.id)).unwrap()[0].preview.is_some());
        assert!(!ws.thumbnail_bytes(&asset.hash).unwrap().is_empty());
    }
    #[test]
    fn schema_three_backup_restores_and_migrates_with_safety_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("old")).unwrap();
        ws.rename("Old studio").unwrap();
        ws.connection().unwrap().execute_batch("DROP TABLE post_markdown_media;DROP TABLE post_media;DROP TABLE posts;DROP TABLE project_sync;DROP TABLE shared_decal_codes;DROP TABLE roblox_group_revenue_passes;DROP TABLE roblox_group_revenue_sales;DROP TABLE roblox_group_revenue_groups;DROP TABLE media_library;DROP TABLE overlay_templates;DROP TABLE markdown_presets;DROP TABLE overlay_render_versions;DROP INDEX assets_trash;ALTER TABLE assets DROP COLUMN trash_id;ALTER TABLE managed_files DROP COLUMN local_available;DROP TABLE content_leases;DROP TABLE upload_attempts;DROP TABLE upload_batch_items;DROP TABLE upload_passes;DROP TABLE upload_jobs;DROP TABLE upload_batches;DROP TABLE image_validation;DROP TABLE roblox_uploads;DROP TABLE publishing_media;DROP TABLE publishing_drafts;DROP TABLE asset_versions;DROP TABLE asset_placements;DROP TABLE asset_folders;DROP TABLE trash_entries;PRAGMA user_version=3;").unwrap();
        ws.connection()
            .unwrap()
            .execute_batch("DROP TABLE statistics_snapshots; DROP TABLE statistics_sales;")
            .unwrap();
        let path = dir.path().join("old.apexbackup");
        ws.backup(&path).unwrap();
        let restored = Workspace::open(dir.path().join("new")).unwrap();
        restored.restore_backup(&path).unwrap();
        assert_eq!(
            restored.info().unwrap().schema_version,
            crate::SCHEMA_VERSION
        );
        assert_eq!(restored.info().unwrap().name, "Old studio");
        let migrated = Workspace::open(dir.path().join("old")).unwrap();
        assert_eq!(
            migrated.info().unwrap().schema_version,
            crate::SCHEMA_VERSION
        );
        assert!(std::fs::read_dir(migrated.root.join("backups"))
            .unwrap()
            .next()
            .is_some());
    }
}
