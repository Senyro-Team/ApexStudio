use super::{storage, Workspace};
use apex_core::{Error, PublishingDraft, PublishingImage, PublishingInput, PublishingPreview};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostSummary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub revision: u64,
    pub liveries: u32,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostSourceStatus {
    pub folder_id: String,
    pub vehicle: String,
    pub state: String,
}
impl Workspace {
    pub fn posts(&self) -> Result<Vec<PostSummary>, Error> {
        let db = self.connection()?;
        let mut q=db.prepare("SELECT id,json_extract(data,'$.title'),updated_at,revision,json_array_length(data,'$.liveries') FROM posts ORDER BY updated_at DESC,id DESC").map_err(storage)?;
        let rows = q
            .query_map([], |r| {
                Ok(PostSummary {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    updated_at: r.get(2)?,
                    revision: r.get::<_, u32>(3)?.into(),
                    liveries: r.get(4)?,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage);
        rows
    }
    pub fn post(&self, id: &str) -> Result<PublishingDraft, Error> {
        let (json, revision): (String, u32) = self
            .connection()?
            .query_row("SELECT data,revision FROM posts WHERE id=?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        Ok(PublishingDraft {
            data: serde_json::from_str(&json).map_err(storage)?,
            revision: revision.into(),
        })
    }
    pub fn create_post(&self) -> Result<String, Error> {
        let id = uuid::Uuid::now_v7().to_string();
        self.connection()?
            .execute(
                "INSERT INTO posts(id,data)VALUES(?1,?2)",
                params![
                    id,
                    serde_json::to_string(&PublishingInput::default()).map_err(storage)?
                ],
            )
            .map_err(storage)?;
        Ok(id)
    }
    pub fn save_post(
        &self,
        id: &str,
        revision: u64,
        data: PublishingInput,
    ) -> Result<PublishingDraft, Error> {
        let gate = self.content_gate();
        let _guard = gate.lock();
        data.validate(false)?;
        for m in &data.media {
            super::validate_texture(&self.root.join("files").join(&m.hash))?;
        }
        let json = serde_json::to_string(&data).map_err(storage)?;
        if json.len() > 256 * 1024 {
            return Err(Error::InvalidContent("This post is too large.".into()));
        }
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(storage)?;
        if tx.execute("UPDATE posts SET data=?2,revision=revision+1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1 AND revision=?3",params![id,json,i64::try_from(revision).map_err(storage)?]).map_err(storage)?!=1{return Err(Error::Conflict);}
        tx.execute("DELETE FROM post_media WHERE post_id=?1", [id])
            .map_err(storage)?;
        for (i, m) in data.media.iter().enumerate() {
            tx.execute(
                "INSERT INTO post_media VALUES(?1,?2,?3,?4)",
                params![id, m.hash, m.name, i as i64],
            )
            .map_err(storage)?;
        }
        // Legacy shadows keep old API readers compatible; they are no longer project package content.
        let legacy: Option<String> = tx
            .query_row(
                "SELECT legacy_project_id FROM posts WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if let Some(project) = legacy {
            tx.execute(
                "UPDATE publishing_drafts SET data=?2,revision=revision+1 WHERE project_id=?1",
                params![project, json],
            )
            .map_err(storage)?;
            tx.execute(
                "DELETE FROM publishing_media WHERE project_id=?1",
                [&project],
            )
            .map_err(storage)?;
            for (i, m) in data.media.iter().enumerate() {
                tx.execute(
                    "INSERT INTO publishing_media VALUES(?1,?2,?3,?4)",
                    params![project, m.hash, m.name, i as i64],
                )
                .map_err(storage)?;
            }
        }
        tx.commit().map_err(storage)?;
        self.post(id)
    }
    pub(super) fn sync_legacy_post(&self, project: &str) -> Result<(), Error> {
        let db = self.connection()?;
        if !db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM publishing_drafts WHERE project_id=?1)",
                [project],
                |r| r.get::<_, bool>(0),
            )
            .map_err(storage)?
        {
            return Ok(());
        }
        let mut db = db;
        let tx = db.transaction().map_err(storage)?;
        tx.execute("INSERT INTO posts(id,data,revision,legacy_project_id) SELECT project_id,data,revision,project_id FROM publishing_drafts WHERE project_id=?1 ON CONFLICT(id) DO UPDATE SET data=excluded.data,revision=excluded.revision,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",[project]).map_err(storage)?;
        tx.execute("DELETE FROM post_media WHERE post_id=?1", [project])
            .map_err(storage)?;
        tx.execute("INSERT INTO post_media SELECT project_id,hash,name,ordinal FROM publishing_media WHERE project_id=?1",[project]).map_err(storage)?;
        tx.commit().map_err(storage)
    }
    pub fn add_post_images(
        &self,
        owner: &str,
        paths: &[PathBuf],
    ) -> Result<Vec<PublishingPreview>, Error> {
        let gate = self.content_gate();
        let _guard = gate.lock();
        if paths.len() > 10 {
            return Err(Error::InvalidContent(
                "Choose up to ten PNG previews.".into(),
            ));
        }
        let mut images = Vec::new();
        for path in paths {
            super::validate_texture(path)?;
            let hash = self.import_file(path)?;
            self.pin_content(owner, std::slice::from_ref(&hash))?;
            images.push(PublishingImage {
                hash,
                name: path
                    .file_name()
                    .ok_or(Error::NotFound)?
                    .to_string_lossy()
                    .into_owned(),
            });
        }
        self.publishing_previews(&images)
    }
    pub fn post_sources(&self, id: &str) -> Result<Vec<PostSourceStatus>, Error> {
        let draft = self.post(id)?;
        Ok(draft
            .data
            .liveries
            .iter()
            .filter_map(|l| {
                l.source.as_ref().map(|source| {
                    let state = match self.livery_uploads(&source.folder_id, &source.account_id) {
                        Err(_) => "missing",
                        Ok(current) => {
                            if current.textures.len() != source.hashes.len()
                                || current.textures.len() != l.sides.len()
                                || current
                                    .textures
                                    .iter()
                                    .zip(&source.hashes)
                                    .zip(&l.sides)
                                    .any(|((t, h), s)| {
                                        t.hash != *h
                                            || t.side != s.side
                                            || t.upload.as_ref().is_none_or(|u| {
                                                u.asset_id.as_deref() != Some(&s.decal_id)
                                                    || u.moderation == "rejected"
                                            })
                                    })
                            {
                                "changed"
                            } else {
                                "current"
                            }
                        }
                    };
                    PostSourceStatus {
                        folder_id: source.folder_id.clone(),
                        vehicle: l.vehicle.clone(),
                        state: state.into(),
                    }
                })
            })
            .collect())
    }
    pub fn export_post(
        &self,
        id: &str,
        revision: u64,
        reviewed: bool,
        destination: &Path,
    ) -> Result<(), Error> {
        let gate = self.content_gate();
        let _guard = gate.lock();
        let draft = self.post(id)?;
        if draft.revision != revision {
            return Err(Error::Conflict);
        }
        draft.data.validate(true)?;
        if !reviewed && self.post_sources(id)?.iter().any(|s| s.state != "current") {
            return Err(Error::InvalidContent("A livery source changed or is unavailable. Review the saved codes and confirm use of this snapshot, or refresh its Roblox IDs.".into()));
        }
        self.write_public_bundle(&draft.data, destination)
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn deleted_sources_keep_post_media_and_require_explicit_snapshot_review() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let project = ws
            .save_project(
                None,
                apex_core::ProjectInput {
                    name: "Sources".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let vehicle = apex_core::vehicle_catalogue().unwrap().vehicles.remove(0);
        let first = dir.path().join("Frontplate.png");
        let second = dir.path().join("Cover.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
            .save(&first)
            .unwrap();
        image::RgbImage::from_pixel(2, 2, image::Rgb([4, 5, 6]))
            .save(&second)
            .unwrap();
        let folder = ws
            .import_livery(
                &project.id,
                None,
                None,
                "Test",
                &vehicle.id,
                &[(
                    first.clone(),
                    "Frontplate.png".into(),
                    Some("Frontplate 1".into()),
                )],
                None,
            )
            .unwrap();
        let texture = ws
            .livery_uploads(&folder, "123")
            .unwrap()
            .textures
            .remove(0);
        ws.claim_decal(&folder, "123", &texture.hash).unwrap();
        ws.save_decal_upload(&super::super::DecalUpload {
            account_id: "123".into(),
            hash: texture.hash.clone(),
            state: "uploaded".into(),
            asset_id: Some("456".into()),
            operation: None,
            moderation: "approved".into(),
            message: String::new(),
        })
        .unwrap();
        let id = ws.create_post().unwrap();
        let images = ws.add_post_images("test", &[first, second]).unwrap();
        let data = PublishingInput {
            title: "Test snapshot".into(),
            description: "Demo post".into(),
            media: images
                .into_iter()
                .map(|m| PublishingImage {
                    hash: m.hash,
                    name: m.name,
                })
                .collect(),
            liveries: vec![apex_core::PublishingLivery {
                source: Some(apex_core::PublishingSource {
                    folder_id: folder.clone(),
                    account_id: "123".into(),
                    hashes: vec![texture.hash.clone()],
                }),
                vehicle: vehicle.name,
                primary_color: "#FFFFFF".into(),
                secondary_color: None,
                opacity: 100,
                sides: vec![apex_core::PublishingSide {
                    side: "Frontplate 1".into(),
                    decal_id: "456".into(),
                }],
            }],
            ..Default::default()
        };
        let draft = ws.save_post(&id, 1, data).unwrap();
        assert_eq!(ws.post_sources(&id).unwrap()[0].state, "current");
        let entry = ws.trash_item(&folder, true).unwrap();
        ws.purge_trash(&entry).unwrap();
        assert!(ws.root.join("files").join(&texture.hash).exists());
        assert_eq!(ws.post_sources(&id).unwrap()[0].state, "missing");
        let output = dir.path().join("post.apexpublish");
        assert!(ws.export_post(&id, draft.revision, false, &output).is_err());
        ws.export_post(&id, draft.revision, true, &output).unwrap();
        assert_eq!(
            ws.post(&id).unwrap().data.liveries[0].sides[0].decal_id,
            "456"
        );
    }
    #[test]
    fn old_project_package_posts_import_as_independent_global_drafts() {
        use std::io::{Read, Write};
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                apex_core::ProjectInput {
                    name: "Legacy package".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let path = dir.path().join("current.apexstd");
        ws.export_project(&p.id, true, true, &path).unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut bytes = Vec::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        manifest["publishing"] = serde_json::to_value(PublishingInput {
            title: "Imported old post".into(),
            ..Default::default()
        })
        .unwrap();
        let old = dir.path().join("legacy.apexstd");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&old).unwrap());
        zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.finish().unwrap();
        let imported = ws.import_package(&old).unwrap();
        assert_eq!(ws.post(&imported).unwrap().data.title, "Imported old post");
        assert_ne!(imported, p.id);
    }
    #[test]
    fn global_drafts_keep_revisions_media_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let id = ws.create_post().unwrap();
        let mut d = ws.post(&id).unwrap();
        d.data.title = "Global post".into();
        for i in 0..2 {
            let path = dir.path().join(format!("{i}.png"));
            image::RgbImage::from_pixel(2, 2, image::Rgb([i, 2, 3]))
                .save(&path)
                .unwrap();
            let m = ws.add_post_images("editor", &[path]).unwrap().remove(0);
            d.data.media.push(PublishingImage {
                hash: m.hash,
                name: m.name,
            });
        }
        ws.save_post(&id, d.revision, d.data.clone()).unwrap();
        assert!(matches!(
            ws.save_post(&id, d.revision, d.data),
            Err(Error::Conflict)
        ));
        assert_eq!(ws.posts().unwrap().len(), 1);
        let path = dir.path().join("backup.apexbackup");
        ws.backup(&path).unwrap();
        let restored = Workspace::open(dir.path().join("restore")).unwrap();
        restored.restore_backup(&path).unwrap();
        assert_eq!(restored.post(&id).unwrap().data.media.len(), 2);
    }
    #[test]
    fn legacy_drafts_migrate_with_media_order_and_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                apex_core::ProjectInput {
                    name: "Old".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let d = PublishingInput {
            title: "Old post".into(),
            ..Default::default()
        };
        ws.save_publishing(&p.id, 0, d).unwrap();
        ws.connection()
            .unwrap()
            .execute_batch("DROP TABLE post_media;DROP TABLE posts;PRAGMA user_version=8;")
            .unwrap();
        let reopened = Workspace::open(dir.path().join("ws")).unwrap();
        assert_eq!(reopened.post(&p.id).unwrap().data.title, "Old post");
        let package = dir.path().join("project.apexstd");
        reopened
            .export_project(&p.id, true, true, &package)
            .unwrap();
        let copy = reopened.import_package(&package).unwrap();
        assert!(reopened.posts().unwrap().iter().all(|x| x.id != copy));
    }
}
