use super::{storage, Workspace};
use apex_core::{Error, Note, ProjectInput, WorkspaceRepository};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Blob {
    hash: String,
    size: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackedAsset {
    #[serde(default)]
    id: String,
    name: String,
    hash: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    version: u32,
    kind: String,
    project: Option<ProjectInput>,
    notes: Vec<Note>,
    assets: Vec<PackedAsset>,
    blobs: Vec<Blob>,
    database_sha256: Option<String>,
    #[serde(default)]
    folders: Vec<apex_core::AssetFolder>,
    #[serde(default)]
    placements: Vec<apex_core::Placement>,
    #[serde(default)]
    versions: Vec<apex_core::AssetVersion>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackagePreview {
    pub title: String,
    pub assets: usize,
    pub notes: usize,
    pub bytes: u64,
    pub kind: String,
}
fn validate_graph(m: &Manifest) -> Result<(), Error> {
    if m.folders.len() > 2000 || m.placements.len() > 10000 || m.versions.len() > 10000 {
        return Err(invalid("The asset structure is too large."));
    }
    let folders: HashMap<_, _> = m.folders.iter().map(|f| (f.id.as_str(), f)).collect();
    if folders.len() != m.folders.len() {
        return Err(invalid("Duplicate folder IDs."));
    }
    let hashes: HashSet<_> = m.blobs.iter().map(|b| b.hash.as_str()).collect();
    let ids: HashSet<_> = m.assets.iter().map(|a| a.id.as_str()).collect();
    if m.version == 2 && (ids.len() != m.assets.len() || ids.contains("")) {
        return Err(invalid("Invalid asset IDs."));
    }
    for f in &m.folders {
        super::assets::validate_folder_name(&f.name)?;
        if !["folder", "livery"].contains(&f.kind.as_str())
            || f.id.is_empty()
            || f.id.len() > 100
            || f.vehicle_id.as_ref().is_some_and(|v| v.len() > 100)
        {
            return Err(invalid("Invalid folder metadata."));
        }
        if f.kind == "livery" && f.vehicle_id.is_none() {
            return Err(invalid("Livery vehicle is missing."));
        }
        if let Some(hash) = &f.cover_hash {
            if !hashes.contains(hash.as_str()) {
                return Err(invalid("Cover image is missing."));
            }
        }
        let mut visited = HashSet::from([f.id.as_str()]);
        let mut parent = f.parent_id.as_deref();
        while let Some(id) = parent {
            let p = folders
                .get(id)
                .ok_or_else(|| invalid("Unknown parent folder."))?;
            if p.kind != "folder" || !visited.insert(id) {
                return Err(invalid("Invalid folder hierarchy."));
            }
            parent = p.parent_id.as_deref();
        }
    }
    let mut assigned = HashSet::new();
    let mut slots = HashSet::new();
    for p in &m.placements {
        if !ids.contains(p.asset_id.as_str()) || !assigned.insert(&p.asset_id) {
            return Err(invalid("Invalid asset placement."));
        }
        if let Some(fid) = &p.folder_id {
            if !folders.contains_key(fid.as_str()) {
                return Err(invalid("Unknown asset folder."));
            }
        }
        if let Some(slot) = &p.slot {
            if slot.len() > 60 || p.folder_id.is_none() || !slots.insert((&p.folder_id, slot)) {
                return Err(invalid("Duplicate or invalid slot."));
            }
        }
    }
    for v in &m.versions {
        if !ids.contains(v.asset_id.as_str())
            || !hashes.contains(v.hash.as_str())
            || v.name.len() > 1024
            || v.created_at.len() > 60
        {
            return Err(invalid("Invalid version metadata."));
        }
    }
    Ok(())
}
struct Checked {
    dir: tempfile::TempDir,
    manifest: Manifest,
}
fn invalid(s: &str) -> Error {
    Error::InvalidContent(s.into())
}
fn file_hash(path: &Path) -> Result<String, Error> {
    let mut f = fs::File::open(path).map_err(storage)?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = f.read(&mut buf).map_err(storage)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn checked(path: &Path) -> Result<Checked, Error> {
    let mut zip = ZipArchive::new(fs::File::open(path).map_err(storage)?).map_err(storage)?;
    if zip.len() > 10002 {
        return Err(invalid("This archive contains too many files."));
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(storage)?;
        let name = entry.name();
        if !names.insert(name.to_owned())
            || entry.is_dir()
            || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
            || !(name == "manifest.json"
                || name == "workspace.sqlite3"
                || name
                    .strip_prefix("files/")
                    .is_some_and(apex_core::valid_hash))
        {
            return Err(invalid(
                "The archive contains an invalid or duplicate path.",
            ));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| invalid("Archive size overflow."))?;
        if entry.size() > 512 * 1024 * 1024 || total > MAX_TOTAL {
            return Err(invalid(
                "The archive exceeds the 2 GB import limit or 512 MB file limit.",
            ));
        }
    }
    let manifest: Manifest = {
        let entry = zip.by_name("manifest.json").map_err(storage)?;
        if entry.size() > 8 * 1024 * 1024 {
            return Err(invalid("The manifest is too large."));
        }
        serde_json::from_reader(entry.take(8 * 1024 * 1024 + 1)).map_err(storage)?
    };
    if ![1, 2].contains(&manifest.version)
        || !["project", "workspace"].contains(&manifest.kind.as_str())
        || manifest.blobs.len() > 10000
        || manifest.notes.len() > 1000
    {
        return Err(invalid("Unsupported archive version or content limits."));
    }
    validate_graph(&manifest)?;
    let mut expected = HashSet::from(["manifest.json".to_string()]);
    let dir = tempfile::tempdir().map_err(storage)?;
    fs::create_dir(dir.path().join("files")).map_err(storage)?;
    for blob in &manifest.blobs {
        if !apex_core::valid_hash(&blob.hash) || !expected.insert(format!("files/{}", blob.hash)) {
            return Err(invalid("Invalid or duplicate blob hash."));
        }
        let mut entry = zip
            .by_name(&format!("files/{}", blob.hash))
            .map_err(storage)?;
        if entry.size() != blob.size {
            return Err(invalid("File size does not match the manifest."));
        }
        let path = dir.path().join("files").join(&blob.hash);
        let mut file = fs::File::create(&path).map_err(storage)?;
        let size =
            std::io::copy(&mut (&mut entry).take(blob.size + 1), &mut file).map_err(storage)?;
        if size != blob.size || file_hash(&path)? != blob.hash {
            return Err(invalid("An archive file failed its integrity check."));
        }
    }
    if manifest.kind == "project" {
        apex_core::validate_project(
            manifest
                .project
                .clone()
                .ok_or_else(|| invalid("Project metadata is missing."))?,
        )?;
        for asset in &manifest.assets {
            if asset.name.is_empty()
                || asset.name.chars().count() > 255
                || asset
                    .name
                    .chars()
                    .any(|c| c.is_control() || c == '/' || c == '\\')
                || !expected.contains(&format!("files/{}", asset.hash))
            {
                return Err(invalid("Invalid asset metadata."));
            }
        }
        for note in &manifest.notes {
            apex_core::validate_note(&note.title, &note.document, &note.markdown)?;
            for hash in apex_core::note_images(&note.document) {
                if !expected.contains(&format!("files/{hash}")) {
                    return Err(invalid("A note image is missing from the package."));
                }
            }
        }
    } else {
        expected.insert("workspace.sqlite3".into());
        let mut entry = zip.by_name("workspace.sqlite3").map_err(storage)?;
        let path = dir.path().join("workspace.sqlite3");
        let mut out = fs::File::create(&path).map_err(storage)?;
        std::io::copy(&mut entry, &mut out).map_err(storage)?;
        if manifest.database_sha256.as_deref() != Some(file_hash(&path)?.as_str()) {
            return Err(invalid("The backup database failed its integrity check."));
        }
        let db = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(storage)?;
        db.execute_batch("PRAGMA trusted_schema=OFF;")
            .map_err(storage)?;
        let check: String = db
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(storage)?;
        let version: u32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(storage)?;
        if check != "ok" || !(3..=super::SCHEMA_VERSION).contains(&version) {
            return Err(invalid(
                "The backup database is damaged or uses a different schema version.",
            ));
        }
        let allowed = HashSet::from([
            "workspace_settings",
            "managed_files",
            "projects",
            "assets",
            "notes",
            "note_images",
            "asset_folders",
            "asset_placements",
            "asset_versions",
        ]);
        let mut stmt=db.prepare("SELECT name,type FROM sqlite_master WHERE type IN ('table','view','trigger') AND name NOT LIKE 'sqlite_%'").map_err(storage)?;
        let schema = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(storage)?;
        for row in schema {
            let (name, kind) = row.map_err(storage)?;
            if kind != "table" || !allowed.contains(name.as_str()) {
                return Err(invalid("Unexpected database schema in backup."));
            }
        }
        if db
            .prepare("PRAGMA foreign_key_check")
            .map_err(storage)?
            .exists([])
            .map_err(storage)?
        {
            return Err(invalid("The backup has broken file or project references."));
        }
        let mut stmt = db
            .prepare("SELECT hash,size FROM managed_files")
            .map_err(storage)?;
        for row in stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(storage)?
        {
            let (h, s) = row.map_err(storage)?;
            if !manifest
                .blobs
                .iter()
                .any(|b| b.hash == h && i64::try_from(b.size).ok() == Some(s))
            {
                return Err(invalid("Backup is missing a managed file."));
            }
        }
    }
    if names != expected {
        return Err(invalid("The archive contains undeclared files."));
    }
    Ok(Checked { dir, manifest })
}
fn preview(c: &Checked) -> PackagePreview {
    PackagePreview {
        title: c
            .manifest
            .project
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Workspace backup".into()),
        assets: c.manifest.assets.len(),
        notes: c.manifest.notes.len(),
        bytes: c.manifest.blobs.iter().map(|b| b.size).sum(),
        kind: c.manifest.kind.clone(),
    }
}
impl Workspace {
    pub fn preview_package(&self, path: &Path) -> Result<PackagePreview, Error> {
        Ok(preview(&checked(path)?))
    }
    fn write_archive(
        &self,
        manifest: &Manifest,
        database: Option<&Path>,
        destination: &Path,
    ) -> Result<(), Error> {
        let total = manifest.blobs.iter().try_fold(0u64, |sum, blob| {
            if blob.size > 512 * 1024 * 1024 {
                return Err(invalid("Each archive file must be smaller than 512 MB."));
            }
            sum.checked_add(blob.size)
                .ok_or_else(|| invalid("Archive size overflow."))
        })?;
        if total > MAX_TOTAL - 16 * 1024 * 1024
            || manifest.blobs.len() > 10_000
            || manifest.notes.len() > 1_000
        {
            return Err(invalid(
                "This selection exceeds the archive limits. Export a smaller project.",
            ));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| invalid("Choose a destination folder."))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(storage)?;
        {
            let mut zip = ZipWriter::new(temporary.as_file_mut());
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("manifest.json", options).map_err(storage)?;
            zip.write_all(&serde_json::to_vec(manifest).map_err(storage)?)
                .map_err(storage)?;
            for blob in &manifest.blobs {
                let path = self.root.join("files").join(&blob.hash);
                if file_hash(&path)? != blob.hash {
                    return Err(invalid("A managed file failed its integrity check."));
                }
                zip.start_file(format!("files/{}", blob.hash), options)
                    .map_err(storage)?;
                std::io::copy(&mut fs::File::open(path).map_err(storage)?, &mut zip)
                    .map_err(storage)?;
            }
            if let Some(path) = database {
                zip.start_file("workspace.sqlite3", options)
                    .map_err(storage)?;
                std::io::copy(&mut fs::File::open(path).map_err(storage)?, &mut zip)
                    .map_err(storage)?;
            }
            zip.finish().map_err(storage)?;
        }
        temporary.as_file().sync_all().map_err(storage)?;
        temporary.persist_noclobber(destination).map_err(storage)?;
        Ok(())
    }
    pub fn export_project(
        &self,
        id: &str,
        embedded: bool,
        include_notes: bool,
        destination: &Path,
    ) -> Result<(), Error> {
        let project = self.project(id)?;
        let assets = if embedded {
            self.assets(Some(id))?
                .into_iter()
                .map(|a| PackedAsset {
                    id: a.id,
                    name: a.name,
                    hash: a.hash,
                })
                .collect()
        } else {
            Vec::new()
        };
        let notes = if embedded && include_notes {
            self.notes(Some(id))?
        } else {
            Vec::new()
        };
        let mut folders = self.folders(id)?;
        for folder in &mut folders {
            folder.preview = None;
            if !embedded {
                folder.cover_hash = None;
            }
        }
        let board = self.board(id)?;
        let placements = if embedded {
            board.placements
        } else {
            Vec::new()
        };
        let mut versions = Vec::new();
        if embedded {
            for asset in &assets {
                versions.extend(self.versions(&asset.id)?);
            }
        }
        let mut hashes = HashSet::new();
        for folder in &folders {
            if let Some(hash) = &folder.cover_hash {
                hashes.insert(hash.clone());
            }
        }
        for version in &versions {
            hashes.insert(version.hash.clone());
        }
        for asset in &assets {
            hashes.insert(asset.hash.clone());
        }
        for note in &notes {
            for hash in apex_core::note_images(&note.document) {
                hashes.insert(hash);
            }
        }
        let blobs = hashes
            .into_iter()
            .map(|hash| {
                let size = fs::metadata(self.root.join("files").join(&hash))
                    .map_err(storage)?
                    .len();
                Ok(Blob { hash, size })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let m = Manifest {
            version: 2,
            kind: "project".into(),
            project: Some(project.fields),
            notes,
            assets,
            blobs,
            database_sha256: None,
            folders,
            placements,
            versions,
        };
        self.write_archive(&m, None, destination)
    }
    pub fn import_package(&self, path: &Path) -> Result<String, Error> {
        let c = checked(path)?;
        if c.manifest.kind != "project" {
            return Err(invalid("Use Restore backup for a workspace backup."));
        }
        // Immutable blobs may remain unreferenced after interruption; visible records commit together.
        for blob in &c.manifest.blobs {
            self.import_file(&c.dir.path().join("files").join(&blob.hash))?;
            self.prepare_thumbnail(&blob.hash);
        }
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(storage)?;
        let id = uuid::Uuid::now_v7().to_string();
        let fields = c
            .manifest
            .project
            .ok_or_else(|| invalid("Missing project."))?;
        tx.execute(
            "INSERT INTO projects(id,metadata) VALUES(?1,?2)",
            rusqlite::params![id, serde_json::to_string(&fields).map_err(storage)?],
        )
        .map_err(storage)?;
        let folder_ids: HashMap<_, _> = c
            .manifest
            .folders
            .iter()
            .map(|f| (f.id.clone(), uuid::Uuid::now_v7().to_string()))
            .collect();
        let mut pending: Vec<_> = c.manifest.folders.iter().collect();
        let mut done = HashSet::new();
        while !pending.is_empty() {
            let before = pending.len();
            let mut next = Vec::new();
            for f in pending {
                if f.parent_id.as_ref().is_some_and(|id| !done.contains(id)) {
                    next.push(f);
                    continue;
                }
                let fid = &folder_ids[&f.id];
                let parent = f.parent_id.as_ref().map(|p| folder_ids[p].clone());
                tx.execute("INSERT INTO asset_folders(id,project_id,parent_id,name,kind,vehicle_id,cover_hash)VALUES(?1,?2,?3,?4,?5,?6,?7)",rusqlite::params![fid,id,parent,f.name,f.kind,f.vehicle_id,f.cover_hash]).map_err(storage)?;
                done.insert(f.id.clone());
            }
            if next.len() == before {
                return Err(invalid("Folder hierarchy contains a cycle."));
            }
            pending = next;
        }
        let mut asset_ids = HashMap::new();
        for asset in c.manifest.assets {
            let aid = uuid::Uuid::now_v7().to_string();
            asset_ids.insert(asset.id.clone(), aid.clone());
            tx.execute(
                "INSERT INTO assets(id,project_id,name,hash) VALUES(?1,?2,?3,?4)",
                rusqlite::params![aid, id, asset.name, asset.hash],
            )
            .map_err(storage)?;
        }
        for placement in c.manifest.placements {
            let aid = asset_ids
                .get(&placement.asset_id)
                .ok_or_else(|| invalid("Unknown asset in placement."))?;
            let fid = placement
                .folder_id
                .as_ref()
                .map(|f| {
                    folder_ids
                        .get(f)
                        .cloned()
                        .ok_or_else(|| invalid("Unknown folder."))
                })
                .transpose()?;
            tx.execute(
                "INSERT INTO asset_placements(asset_id,folder_id,slot)VALUES(?1,?2,?3)",
                rusqlite::params![aid, fid, placement.slot],
            )
            .map_err(storage)?;
        }
        for version in c.manifest.versions {
            let aid = asset_ids
                .get(&version.asset_id)
                .ok_or_else(|| invalid("Unknown version asset."))?;
            tx.execute("INSERT INTO asset_versions(id,asset_id,hash,name,created_at)VALUES(?1,?2,?3,?4,?5)",rusqlite::params![uuid::Uuid::now_v7().to_string(),aid,version.hash,version.name,version.created_at]).map_err(storage)?;
        }
        for note in c.manifest.notes {
            let nid = uuid::Uuid::now_v7().to_string();
            tx.execute("INSERT INTO notes(id,project_id,title,document,markdown,archived) VALUES(?1,?2,?3,?4,?5,?6)",rusqlite::params![nid,id,note.title,note.document.to_string(),note.markdown,note.archived]).map_err(storage)?;
            for hash in apex_core::note_images(&note.document) {
                tx.execute(
                    "INSERT INTO note_images(note_id,hash) VALUES(?1,?2)",
                    rusqlite::params![nid, hash],
                )
                .map_err(storage)?;
            }
        }
        tx.commit().map_err(storage)?;
        Ok(id)
    }
    pub fn backup(&self, destination: &Path) -> Result<(), Error> {
        let dir = tempfile::tempdir().map_err(storage)?;
        let path = dir.path().join("workspace.sqlite3");
        let source = self.connection()?;
        let mut target = rusqlite::Connection::open(&path).map_err(storage)?;
        rusqlite::backup::Backup::new(&source, &mut target)
            .map_err(storage)?
            .run_to_completion(128, Duration::from_millis(5), None)
            .map_err(storage)?;
        drop(target);
        let snapshot = rusqlite::Connection::open(&path).map_err(storage)?;
        let mut stmt = snapshot
            .prepare("SELECT hash,size FROM managed_files ORDER BY hash")
            .map_err(storage)?;
        let blobs = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?
            .into_iter()
            .map(|(hash, size)| {
                Ok(Blob {
                    hash,
                    size: u64::try_from(size).map_err(storage)?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let m = Manifest {
            version: 1,
            kind: "workspace".into(),
            project: None,
            notes: Vec::new(),
            assets: Vec::new(),
            blobs,
            database_sha256: Some(file_hash(&path)?),
            folders: Vec::new(),
            placements: Vec::new(),
            versions: Vec::new(),
        };
        self.write_archive(&m, Some(&path), destination)
    }
    pub fn restore_backup(&self, path: &Path) -> Result<PathBuf, Error> {
        let c = checked(path)?;
        if c.manifest.kind != "workspace" {
            return Err(invalid("Choose a workspace backup."));
        }
        let safety = self.root.join("backups").join(format!(
            "before-restore-{}.apexbackup",
            uuid::Uuid::now_v7()
        ));
        self.backup(&safety)?;
        for blob in &c.manifest.blobs {
            self.import_file(&c.dir.path().join("files").join(&blob.hash))?;
            self.prepare_thumbnail(&blob.hash);
        }
        let staged = Workspace::open(c.dir.path())?;
        drop(staged);
        let source =
            rusqlite::Connection::open(c.dir.path().join("workspace.sqlite3")).map_err(storage)?;
        let mut target = self.connection()?;
        rusqlite::backup::Backup::new(&source, &mut target)
            .map_err(storage)?
            .run_to_completion(128, Duration::from_millis(5), None)
            .map_err(storage)?;
        let _ = self.info()?;
        Ok(safety)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn project_roundtrip_is_a_copy_and_backup_restores_database_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("one")).unwrap();
        let p = ws
            .save_project(
                None,
                ProjectInput {
                    name: "Livery".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let file = dir.path().join("source.txt");
        fs::write(&file, b"original").unwrap();
        ws.import_project_files(&p.id, &[file]).unwrap();
        let n = ws.create_note(Some(&p.id)).unwrap();
        let image = dir.path().join("note.png");
        image::RgbImage::new(2, 2).save(&image).unwrap();
        let hash = ws.attach_note_image(&n.id, &image).unwrap();
        let document = serde_json::json!({"type":"doc","content":[{"type":"image","attrs":{"src":format!("apex-image:{hash}")}}]});
        ws.save_note(&n.id, n.revision, "Notes", &document, "image")
            .unwrap();
        let package = dir.path().join("project.apexstd");
        ws.export_project(&p.id, true, true, &package).unwrap();
        let other = Workspace::open(dir.path().join("two")).unwrap();
        let id = other.import_package(&package).unwrap();
        assert_ne!(id, p.id);
        assert_eq!(other.project(&id).unwrap().asset_count, 1);
        assert_eq!(other.notes(Some(&id)).unwrap()[0].title, "Notes");
        let imported_note = other.notes(Some(&id)).unwrap().remove(0);
        assert!(other
            .note_image(&imported_note.id, &hash)
            .unwrap()
            .starts_with("data:image/png;base64,"));
        let metadata = dir.path().join("metadata.apexstd");
        ws.export_project(&p.id, false, true, &metadata).unwrap();
        let preview = ws.preview_package(&metadata).unwrap();
        assert_eq!(preview.assets, 0);
        assert_eq!(preview.notes, 0);
        let backup = dir.path().join("all.apexbackup");
        ws.backup(&backup).unwrap();
        let restored = Workspace::open(dir.path().join("three")).unwrap();
        restored.rename("Changed").unwrap();
        let safety = restored.restore_backup(&backup).unwrap();
        assert!(safety.exists());
        assert_eq!(restored.info().unwrap().name, "My workspace");
        assert_eq!(restored.project(&p.id).unwrap().asset_count, 1);
        assert!(restored
            .note_image(&n.id, &hash)
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }
    #[test]
    fn corrupt_files_are_rejected_before_records_are_imported() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let path = dir.path().join("corrupt.apexstd");
        let hash = "0".repeat(64);
        let manifest = Manifest {
            version: 1,
            kind: "project".into(),
            project: Some(ProjectInput {
                name: "Invalid".into(),
                ..Default::default()
            }),
            notes: Vec::new(),
            assets: vec![PackedAsset {
                id: String::new(),
                name: "x.txt".into(),
                hash: hash.clone(),
            }],
            blobs: vec![Blob {
                hash: hash.clone(),
                size: 3,
            }],
            database_sha256: None,
            folders: Vec::new(),
            placements: Vec::new(),
            versions: Vec::new(),
        };
        let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.start_file(format!("files/{hash}"), SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();
        assert!(ws.import_package(&path).is_err());
        assert!(ws.projects().unwrap().is_empty());
    }
    #[test]
    fn legacy_version_one_packages_import_as_flat_assets() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let path = dir.path().join("legacy.apexstd");
        let hash = format!("{:x}", Sha256::digest(b"v1"));
        let manifest = serde_json::json!({"version":1,"kind":"project","project":{"name":"Legacy","description":"","tags":[],"vehicles":[],"referenceLinks":[],"deadline":null},"notes":[],"assets":[{"name":"legacy.txt","hash":hash}],"blobs":[{"hash":hash,"size":2}],"databaseSha256":null});
        let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.start_file(format!("files/{hash}"), SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"v1").unwrap();
        zip.finish().unwrap();
        let id = ws.import_package(&path).unwrap();
        assert_eq!(ws.assets(Some(&id)).unwrap()[0].name, "legacy.txt");
        assert!(ws.folders(&id).unwrap().is_empty());
    }
    #[test]
    fn rejects_traversal_before_import() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.apexstd");
        let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file("../escape", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"no").unwrap();
        zip.finish().unwrap();
        assert!(checked(&path).is_err());
        assert!(!dir.path().join("escape").exists());
    }
}
