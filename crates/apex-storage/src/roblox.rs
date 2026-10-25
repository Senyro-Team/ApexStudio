use super::{storage, Workspace};
use apex_core::Error;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DecalUpload {
    pub account_id: String,
    pub hash: String,
    pub state: String,
    pub operation: Option<String>,
    pub asset_id: Option<String>,
    pub moderation: String,
    pub message: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use apex_core::{
        ProjectInput, PublishingInput, PublishingLivery, PublishingSide, PublishingSource,
    };
    #[test]
    fn decal_upload_accepts_8000_square_and_rejects_dimensions_above_8000() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let allowed = dir.path().join("8000.png");
        image::RgbImage::new(8000, 8000).save(&allowed).unwrap();
        let hash = ws.import_file(&allowed).unwrap();
        assert!(!ws.decal_bytes(&hash).unwrap().is_empty());
        for (width, height) in [(8001, 1), (1, 8001)] {
            let path = dir.path().join(format!("{width}x{height}.png"));
            image::RgbImage::new(width, height).save(&path).unwrap();
            let hash = ws.import_file(&path).unwrap();
            assert!(ws.decal_bytes(&hash).is_err());
        }
    }
    #[test]
    fn exact_texture_claims_survive_backup_and_new_versions_require_new_uploads() {
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
        let vehicle = apex_core::vehicle_catalogue().unwrap().vehicles.remove(0);
        let path = dir.path().join("Left1.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
            .save(&path)
            .unwrap();
        let folder = ws
            .import_livery(
                &p.id,
                None,
                None,
                "Patrol",
                &vehicle.id,
                &[(path.clone(), "Left1.png".into(), Some("Left 1".into()))],
                None,
            )
            .unwrap();
        let plan = ws.livery_uploads(&folder, "123").unwrap();
        let hash = plan.textures[0].hash.clone();
        assert!(ws.claim_decal(&folder, "123", &hash).unwrap());
        assert!(!ws.claim_decal(&folder, "123", &hash).unwrap());
        let mut upload = ws.decal_upload("123", &hash).unwrap().unwrap();
        upload.state = "processing".into();
        upload.operation = Some("operations/test".into());
        ws.save_decal_upload(&upload).unwrap();
        assert!(!ws.claim_decal(&folder, "123", &hash).unwrap());
        upload.state = "uploaded".into();
        upload.asset_id = Some("456".into());
        upload.moderation = "approved".into();
        ws.save_decal_upload(&upload).unwrap();
        let mut data = PublishingInput {
            title: "Patrol".into(),
            description: "Public post".into(),
            liveries: vec![PublishingLivery {
                source: Some(PublishingSource {
                    folder_id: folder.clone(),
                    account_id: "123".into(),
                    hashes: vec![hash.clone()],
                }),
                vehicle: vehicle.name,
                primary_color: "#FFFFFF".into(),
                secondary_color: None,
                opacity: 100,
                sides: vec![PublishingSide {
                    side: "Left 1".into(),
                    decal_id: "456".into(),
                }],
            }],
            ..Default::default()
        };
        for n in 0..2 {
            let path = dir.path().join(format!("preview{n}.png"));
            image::RgbImage::from_pixel(2, 2, image::Rgb([n, 3, 4]))
                .save(&path)
                .unwrap();
            let preview = ws.add_publishing_images(&p.id, &[path]).unwrap().remove(0);
            data.media.push(apex_core::PublishingImage {
                hash: preview.hash,
                name: preview.name,
            });
        }
        ws.save_publishing(&p.id, 0, data).unwrap();
        let public = dir.path().join("public.apexpublish");
        ws.export_publishing(&p.id, &public).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&public).unwrap()).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_reader(zip.by_name("public.json").unwrap()).unwrap();
        assert!(manifest["liveries"][0].get("source").is_none());
        assert_eq!(manifest["liveries"][0]["sides"][0]["decalId"], "456");
        let backup = dir.path().join("test.apexbackup");
        ws.backup(&backup).unwrap();
        let restored = Workspace::open(dir.path().join("restored")).unwrap();
        restored.restore_backup(&backup).unwrap();
        assert_eq!(
            restored
                .decal_upload("123", &hash)
                .unwrap()
                .unwrap()
                .asset_id
                .as_deref(),
            Some("456")
        );
        assert!(ws.decal_upload("999", &hash).unwrap().is_none());
        image::RgbImage::from_pixel(2, 2, image::Rgb([9, 2, 3]))
            .save(&path)
            .unwrap();
        ws.import_livery(
            &p.id,
            None,
            Some(&folder),
            "Patrol",
            &vehicle.id,
            &[(path, "Left1.png".into(), Some("Left 1".into()))],
            None,
        )
        .unwrap();
        let revised = ws.livery_uploads(&folder, "123").unwrap();
        assert!(revised.textures[0].upload.is_none());
        assert!(ws
            .export_publishing(&p.id, &dir.path().join("stale.apexpublish"))
            .is_err());
        assert!(ws.claim_decal(&folder, "123", &hash).is_err());
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadTexture {
    pub asset_id: String,
    pub name: String,
    pub hash: String,
    pub side: String,
    pub upload: Option<DecalUpload>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveryUploads {
    pub folder_id: String,
    pub name: String,
    pub vehicle: String,
    pub account_id: String,
    pub textures: Vec<UploadTexture>,
}
impl Workspace {
    pub fn decal_upload(&self, account: &str, hash: &str) -> Result<Option<DecalUpload>, Error> {
        self.connection()?.query_row("SELECT state,operation,asset_id,moderation,message FROM roblox_uploads WHERE account_id=?1 AND hash=?2",params![account,hash],|r| Ok(DecalUpload{account_id:account.into(),hash:hash.into(),state:r.get(0)?,operation:r.get(1)?,asset_id:r.get(2)?,moderation:r.get(3)?,message:r.get(4)?})).optional().map_err(storage)
    }
    pub fn livery_uploads(&self, folder: &str, account: &str) -> Result<LiveryUploads, Error> {
        let f = self.folder(folder)?;
        if f.kind != "livery" {
            return Err(Error::InvalidContent("Choose a livery folder.".into()));
        }
        let board = self.board(&f.project_id)?;
        let mut textures = Vec::new();
        for p in board
            .placements
            .iter()
            .filter(|p| p.folder_id.as_deref() == Some(folder))
        {
            let Some(side) = &p.slot else { continue };
            let a = board
                .assets
                .iter()
                .find(|a| a.id == p.asset_id)
                .ok_or(Error::NotFound)?;
            textures.push(UploadTexture {
                asset_id: a.id.clone(),
                name: a.name.clone(),
                hash: a.hash.clone(),
                side: side.clone(),
                upload: self.decal_upload(account, &a.hash)?,
            });
        }
        textures.sort_by(|a, b| a.side.cmp(&b.side));
        let vehicle = apex_core::vehicle_catalogue()?
            .vehicles
            .into_iter()
            .find(|v| Some(&v.id) == f.vehicle_id.as_ref())
            .map(|v| v.name)
            .unwrap_or_default();
        Ok(LiveryUploads {
            folder_id: f.id,
            name: f.name,
            vehicle,
            account_id: account.into(),
            textures,
        })
    }
    /// Claims an exact immutable texture before any external POST. An interrupted POST
    /// without an operation is intentionally not retried automatically.
    pub fn claim_decal(&self, folder: &str, account: &str, hash: &str) -> Result<bool, Error> {
        let f = self.folder(folder)?;
        self.editable_project(&f.project_id)?;
        if !self
            .livery_uploads(folder, account)?
            .textures
            .iter()
            .any(|t| t.hash == hash)
        {
            return Err(Error::InvalidContent(
                "This texture changed. Reopen the upload review.".into(),
            ));
        }
        self.decal_bytes(hash)?;
        let changed = self.connection()?.execute("INSERT INTO roblox_uploads(account_id,hash,state) VALUES(?1,?2,'uploading') ON CONFLICT(account_id,hash) DO UPDATE SET state='uploading',message='' WHERE state='failed' AND operation IS NULL AND asset_id IS NULL",params![account,hash]).map_err(storage)?;
        Ok(changed == 1)
    }
    pub fn decal_bytes(&self, hash: &str) -> Result<Vec<u8>, Error> {
        if !apex_core::valid_hash(hash) {
            return Err(Error::NotFound);
        }
        let path = self.root.join("files").join(hash);
        super::validate_texture(&path)?;
        let bytes = std::fs::read(path).map_err(storage)?;
        if format!("{:x}", Sha256::digest(&bytes)) != hash {
            return Err(Error::InvalidContent(
                "Texture integrity check failed.".into(),
            ));
        }
        Ok(bytes)
    }
    pub fn save_decal_upload(&self, upload: &DecalUpload) -> Result<(), Error> {
        self.connection()?.execute("UPDATE roblox_uploads SET state=?3,operation=?4,asset_id=?5,moderation=?6,message=?7 WHERE account_id=?1 AND hash=?2",params![upload.account_id,upload.hash,upload.state,upload.operation,upload.asset_id,upload.moderation,upload.message]).map_err(storage)?;
        Ok(())
    }
}
