use super::{storage, Workspace};
use apex_core::{Error, PublishingDraft, PublishingImage, PublishingInput, PublishingPreview};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
impl Workspace {
    pub fn publishing(&self, project: &str) -> Result<PublishingDraft, Error> {
        self.project(project)?;
        let row: Option<(String, i64)> = self
            .connection()?
            .query_row(
                "SELECT data,revision FROM publishing_drafts WHERE project_id=?1",
                [project],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage)?;
        match row {
            Some((data, revision)) => Ok(PublishingDraft {
                data: serde_json::from_str(&data).map_err(storage)?,
                revision: u64::try_from(revision).map_err(storage)?,
            }),
            None => Ok(PublishingDraft {
                data: PublishingInput::default(),
                revision: 0,
            }),
        }
    }
    pub fn publishing_previews(
        &self,
        images: &[PublishingImage],
    ) -> Result<Vec<PublishingPreview>, Error> {
        images
            .iter()
            .map(|m| {
                let size = self
                    .connection()?
                    .query_row(
                        "SELECT size FROM managed_files WHERE hash=?1",
                        [&m.hash],
                        |r| r.get::<_, i64>(0),
                    )
                    .map_err(storage)?;
                Ok(PublishingPreview {
                    hash: m.hash.clone(),
                    name: m.name.clone(),
                    size: u64::try_from(size).map_err(storage)?,
                    preview: self.thumbnail(&m.hash),
                })
            })
            .collect()
    }
    pub fn add_publishing_images(
        &self,
        project: &str,
        paths: &[PathBuf],
    ) -> Result<Vec<PublishingPreview>, Error> {
        let gate = self.content_gate();
        let _content = gate.lock();
        self.editable_project(project)?;
        if paths.len() > 10 {
            return Err(Error::InvalidContent(
                "Choose up to ten PNG preview images.".into(),
            ));
        }
        let mut images = Vec::new();
        for path in paths {
            super::validate_texture(path)?;
            let hash = self.import_file(path)?;
            let name = path
                .file_name()
                .ok_or(Error::NotFound)?
                .to_string_lossy()
                .into_owned();
            images.push(PublishingImage { hash, name });
        }
        self.publishing_previews(&images)
    }
    pub fn save_publishing(
        &self,
        project: &str,
        revision: u64,
        data: PublishingInput,
    ) -> Result<PublishingDraft, Error> {
        let gate = self.content_gate();
        let _content = gate.lock();
        self.editable_project(project)?;
        data.validate(false)?;
        for m in &data.media {
            super::validate_texture(&self.root.join("files").join(&m.hash))?;
        }
        let json = serde_json::to_string(&data).map_err(storage)?;
        if json.len() > 256 * 1024 {
            return Err(Error::InvalidContent(
                "This publishing draft is too large.".into(),
            ));
        }
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
        let current: Option<i64> = tx
            .query_row(
                "SELECT revision FROM publishing_drafts WHERE project_id=?1",
                [project],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        if current.unwrap_or(0) != i64::try_from(revision).map_err(storage)? {
            return Err(Error::Conflict);
        }
        tx.execute("INSERT INTO publishing_drafts(project_id,data,revision) VALUES(?1,?2,1) ON CONFLICT(project_id) DO UPDATE SET data=excluded.data,revision=revision+1",params![project,json]).map_err(storage)?;
        tx.execute(
            "DELETE FROM publishing_media WHERE project_id=?1",
            [project],
        )
        .map_err(storage)?;
        for (i, m) in data.media.iter().enumerate() {
            tx.execute(
                "INSERT INTO publishing_media(project_id,hash,name,ordinal) VALUES(?1,?2,?3,?4)",
                params![project, m.hash, m.name, i64::try_from(i).map_err(storage)?],
            )
            .map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        self.sync_legacy_post(project)?;
        self.publishing(project)
    }
    pub fn export_publishing(&self, project: &str, destination: &Path) -> Result<(), Error> {
        let gate = self.content_gate();
        let _content = gate.lock();
        let draft = self.publishing(project)?;
        draft.data.validate(true)?;
        for livery in &draft.data.liveries {
            if let Some(source) = &livery.source {
                let current = self.livery_uploads(&source.folder_id, &source.account_id)?;
                if current.textures.len() != source.hashes.len()
                    || current
                        .textures
                        .iter()
                        .zip(&source.hashes)
                        .zip(&livery.sides)
                        .any(|((t, h), s)| {
                            &t.hash != h
                                || t.side != s.side
                                || t.upload.as_ref().is_none_or(|u| {
                                    u.asset_id.as_deref() != Some(&s.decal_id)
                                        || u.moderation == "rejected"
                                })
                        })
                    || current.textures.len() != livery.sides.len()
                {
                    return Err(Error::InvalidContent("A selected livery changed or its upload is unavailable. Refresh its Roblox IDs before exporting.".into()));
                }
            }
        }
        self.write_public_bundle(&draft.data, destination)
    }
    pub(super) fn write_public_bundle(
        &self,
        data: &PublishingInput,
        destination: &Path,
    ) -> Result<(), Error> {
        data.validate(true)?;
        let mut media = Vec::new();
        let mut blobs = Vec::new();
        for m in &data.media {
            let path = self.root.join("files").join(&m.hash);
            super::validate_texture(&path)?;
            let bytes = fs::read(path).map_err(storage)?;
            if format!("{:x}", Sha256::digest(&bytes)) != m.hash {
                return Err(Error::InvalidContent(
                    "A preview image failed its integrity check.".into(),
                ));
            }
            let filename = format!("media/{}.png", m.hash);
            media.push(
                serde_json::json!({"path":filename,"hash":m.hash,"name":m.name,"size":bytes.len()}),
            );
            blobs.push((filename, bytes));
        }
        let generated_at: String = self
            .connection()?
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
                r.get(0)
            })
            .map_err(storage)?;
        let liveries: Vec<_> = data.liveries.iter().map(|l| serde_json::json!({"vehicle":l.vehicle,"primaryColor":l.primary_color,"secondaryColor":l.secondary_color,"opacity":l.opacity,"sides":l.sides})).collect();
        let manifest = serde_json::json!({"version":1,"kind":"apex-publish","targetOrigin":"https://erlcx.com","exportId":uuid::Uuid::now_v7().to_string(),"generatedAt":generated_at,"title":data.title,"description":data.description,"tags":data.tags,"youtubeUrl":data.youtube_url,"liveries":liveries,"media":media});
        let parent = destination.parent().ok_or(Error::NotFound)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(storage)?;
        {
            let mut zip = zip::ZipWriter::new(temporary.as_file_mut());
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("public.json", options).map_err(storage)?;
            zip.write_all(&serde_json::to_vec(&manifest).map_err(storage)?)
                .map_err(storage)?;
            for (name, bytes) in blobs {
                zip.start_file(name, options).map_err(storage)?;
                zip.write_all(&bytes).map_err(storage)?;
            }
            zip.finish().map_err(storage)?;
        }
        temporary.as_file().sync_all().map_err(storage)?;
        temporary.persist_noclobber(destination).map_err(storage)?;
        Ok(())
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use apex_core::{ProjectInput, PublishingLivery, PublishingSide};
    #[test]
    fn publishing_survives_reopen_and_backup_with_revision_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                ProjectInput {
                    name: "Studio".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut d = ws.publishing(&p.id).unwrap();
        assert_eq!(d.revision, 0);
        for i in 0..2 {
            let path = dir.path().join(format!("{i}.png"));
            image::RgbImage::from_pixel(2, 2, image::Rgb([i, 1, 2]))
                .save(&path)
                .unwrap();
            let image = ws.add_publishing_images(&p.id, &[path]).unwrap().remove(0);
            d.data.media.push(PublishingImage {
                hash: image.hash,
                name: image.name,
            });
        }
        d.data.title = "Public title".into();
        d.data.description = "Public description".into();
        d.data.liveries.push(PublishingLivery {
            source: None,
            vehicle: "Vehicle".into(),
            primary_color: "#FFFFFF".into(),
            secondary_color: None,
            opacity: 100,
            sides: vec![PublishingSide {
                side: "Left 1".into(),
                decal_id: "12345".into(),
            }],
        });
        let saved = ws.save_publishing(&p.id, 0, d.data.clone()).unwrap();
        assert!(ws.save_publishing(&p.id, 0, d.data).is_err());
        let export = dir.path().join("test.apexpublish");
        ws.export_publishing(&p.id, &export).unwrap();
        let imported_id = ws.import_post_bundle(&export).unwrap();
        let imported = ws.post(&imported_id).unwrap();
        assert_eq!(imported.data.title, "Public title");
        assert_eq!(imported.data.media.len(), 2);
        assert_eq!(imported.data.liveries.len(), 1);
        assert!(imported.data.liveries[0].source.is_none());
        assert_eq!(imported.data.liveries[0].sides[0].decal_id, "12345");
        let mut zip = zip::ZipArchive::new(fs::File::open(export).unwrap()).unwrap();
        assert_eq!(zip.len(), 3);
        let mut text = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("public.json").unwrap(), &mut text).unwrap();
        assert!(!text.contains(&p.id));
        assert!(!text.contains("notes"));
        let package = dir.path().join("project.apexstd");
        ws.export_project(&p.id, true, true, &package).unwrap();
        let pid = ws.import_package(&package).unwrap();
        assert!(ws.publishing(&pid).unwrap().data.media.is_empty());
        assert_eq!(ws.post(&p.id).unwrap().data.media.len(), 2);
        let reopened = Workspace::open(dir.path().join("ws")).unwrap();
        assert_eq!(reopened.publishing(&p.id).unwrap().revision, saved.revision);
        let backup = dir.path().join("backup.apexbackup");
        ws.backup(&backup).unwrap();
        let restored = Workspace::open(dir.path().join("restored")).unwrap();
        restored.restore_backup(&backup).unwrap();
        assert_eq!(restored.publishing(&p.id).unwrap().data.media.len(), 2);
    }
}
