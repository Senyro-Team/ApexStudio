use super::{storage, Workspace};
use apex_core::Error;
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::Digest;
use std::path::Path;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaLibraryItem {
    pub id: String,
    pub hash: Option<String>,
    pub name: String,
    pub alt_text: String,
    pub image_url: String,
    pub default_link_url: Option<String>,
    pub folder: String,
    pub tags: Vec<String>,
    pub overlay_version_group: Option<String>,
    pub overlay_version: Option<i64>,
    pub overlay_target: Option<String>,
    pub owner_scope: String,
    pub team_id: Option<String>,
    pub provider_image_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub preview: Option<String>,
}

pub struct GeneratedPreview {
    pub name: String,
    pub bytes: Vec<u8>,
    pub source_hash: String,
    pub width: u32,
    pub height: u32,
    pub composition_json: String,
}

fn invalid(message: &str) -> Error {
    Error::InvalidContent(message.into())
}

fn validate_name(value: &str) -> Result<(), Error> {
    let name = value.trim();
    if name.is_empty()
        || name.chars().count() > 160
        || name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(invalid(
            "Enter a media name up to 160 characters without slashes.",
        ));
    }
    Ok(())
}

fn validate_text(value: &str, max: usize, label: &str) -> Result<(), Error> {
    if value.chars().count() > max || value.chars().any(|c| c == '\0') {
        return Err(invalid(label));
    }
    Ok(())
}

fn validate_folder_tags(folder: &str, tags: &[String]) -> Result<(String, Vec<String>), Error> {
    let folder = folder.trim();
    if folder.chars().count() > 80
        || folder
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(invalid(
            "Folder names must be 80 characters or fewer and cannot contain slashes.",
        ));
    }
    if tags.len() > 24 {
        return Err(invalid("Use 24 tags or fewer per image."));
    }
    let mut normalized_tags = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > 32 || tag.chars().any(char::is_control) {
            return Err(invalid("Each tag must be 32 characters or fewer."));
        }
        if !normalized_tags
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(tag))
        {
            normalized_tags.push(tag.to_owned());
        }
    }
    Ok((folder.to_owned(), normalized_tags))
}

fn validate_url(value: &str, required: bool) -> Result<Option<String>, Error> {
    let value = value.trim();
    if value.is_empty() {
        return if required {
            Err(invalid("Enter the public image URL."))
        } else {
            Ok(None)
        };
    }
    if value.len() > 2048 {
        return Err(invalid("Links must be 2048 characters or fewer."));
    }
    let url = url::Url::parse(value).map_err(|_| invalid("Enter a valid HTTPS URL."))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid("Use an HTTPS URL without embedded credentials."));
    }
    Ok(Some(url.to_string()))
}

impl Workspace {
    fn media_library_item(&self, id: &str) -> Result<MediaLibraryItem, Error> {
        let row = self
            .connection()?
            .query_row(
                "SELECT m.id,m.hash,m.name,m.alt_text,m.image_url,m.default_link_url,m.folder,m.tags_json,m.owner_scope,m.team_id,m.provider_image_id,m.created_at,m.updated_at,(SELECT v.version_group_id FROM overlay_render_versions v WHERE v.hash=m.hash ORDER BY v.created_at DESC,v.version DESC LIMIT 1),(SELECT v.version FROM overlay_render_versions v WHERE v.hash=m.hash ORDER BY v.created_at DESC,v.version DESC LIMIT 1),(SELECT v.target FROM overlay_render_versions v WHERE v.hash=m.hash ORDER BY v.created_at DESC,v.version DESC LIMIT 1) FROM media_library m WHERE m.id=?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, Option<String>>(13)?,
                        row.get::<_, Option<i64>>(14)?,
                        row.get::<_, Option<String>>(15)?,
                    ))
                },
            )
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        let preview = row.1.as_deref().and_then(|hash| {
            self.thumbnail_bytes(hash).ok().map(|bytes| {
                format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )
            })
        });
        Ok(MediaLibraryItem {
            id: row.0,
            hash: row.1,
            name: row.2,
            alt_text: row.3,
            image_url: row.4,
            default_link_url: row.5,
            folder: row.6,
            tags: serde_json::from_str(&row.7).map_err(storage)?,
            overlay_version_group: row.13,
            overlay_version: row.14,
            overlay_target: row.15,
            owner_scope: row.8,
            team_id: row.9,
            provider_image_id: row.10,
            created_at: row.11,
            updated_at: row.12,
            preview,
        })
    }

    pub fn media_library(&self) -> Result<Vec<MediaLibraryItem>, Error> {
        let ids = {
            let db = self.connection()?;
            let mut statement = db
                .prepare(
                    "SELECT id FROM media_library ORDER BY updated_at DESC,name COLLATE NOCASE",
                )
                .map_err(storage)?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(storage)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage)?;
            rows
        };
        ids.iter().map(|id| self.media_library_item(id)).collect()
    }

    pub fn media_library_item_by_id(&self, id: &str) -> Result<MediaLibraryItem, Error> {
        self.media_library_item(id)
    }

    pub(super) fn insert_media_item(
        &self,
        hash: Option<&str>,
        name: &str,
        alt_text: &str,
        image_url: &str,
        default_link_url: Option<&str>,
        folder: &str,
        tags: &[String],
    ) -> Result<MediaLibraryItem, Error> {
        validate_name(name)?;
        validate_text(alt_text, 500, "Alt text must be 500 characters or fewer.")?;
        let image_url = validate_url(image_url, hash.is_none())?.unwrap_or_default();
        let default_link_url = validate_url(default_link_url.unwrap_or_default(), false)?;
        let (folder, tags) = validate_folder_tags(folder, tags)?;
        let id = uuid::Uuid::now_v7().to_string();
        self.connection()?
            .execute(
                "INSERT INTO media_library(id,hash,name,alt_text,image_url,default_link_url,folder,tags_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![id, hash, name.trim(), alt_text.trim(), image_url, default_link_url, folder, serde_json::to_string(&tags).map_err(storage)?],
            )
            .map_err(storage)?;
        self.media_library_item(&id)
    }

    pub(super) fn existing_hash_item(&self, hash: &str) -> Result<Option<MediaLibraryItem>, Error> {
        let id = self
            .connection()?
            .query_row(
                "SELECT id FROM media_library WHERE hash=?1",
                [hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage)?;
        id.map(|id| self.media_library_item(&id)).transpose()
    }

    pub fn add_media_library_file(&self, source: &Path) -> Result<MediaLibraryItem, Error> {
        super::validate_texture(source)?;
        let hash = self.import_file(source)?;
        if let Some(existing) = self.existing_hash_item(&hash)? {
            return Ok(existing);
        }
        let name = source.file_name().ok_or(Error::NotFound)?.to_string_lossy();
        let alt = source
            .file_stem()
            .map(|s| s.to_string_lossy().replace(['_', '-'], " "))
            .unwrap_or_else(|| "Image".into());
        self.insert_media_item(Some(&hash), &name, &alt, "", None, "", &[])
    }

    pub fn add_media_library_asset(
        &self,
        project_id: &str,
        asset_id: &str,
    ) -> Result<MediaLibraryItem, Error> {
        let (hash, name): (String, String) = self
            .connection()?
            .query_row(
                "SELECT a.hash,a.name FROM assets a JOIN projects p ON p.id=a.project_id WHERE a.id=?1 AND a.project_id=?2 AND a.trash_id IS NULL AND p.archived=0",
                params![asset_id, project_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        let path = self.root.join("files").join(&hash);
        super::validate_texture(&path)?;
        if let Some(existing) = self.existing_hash_item(&hash)? {
            return Ok(existing);
        }
        let alt = Path::new(&name)
            .file_stem()
            .map(|s| s.to_string_lossy().replace(['_', '-'], " "))
            .unwrap_or_else(|| name.clone());
        self.insert_media_item(Some(&hash), &name, &alt, "", None, "", &[])
    }

    pub fn add_media_library_url(
        &self,
        name: &str,
        alt_text: &str,
        image_url: &str,
        default_link_url: Option<&str>,
        folder: &str,
        tags: &[String],
    ) -> Result<MediaLibraryItem, Error> {
        self.insert_media_item(
            None,
            name,
            alt_text,
            image_url,
            default_link_url,
            folder,
            tags,
        )
    }

    pub fn update_media_library_item(
        &self,
        id: &str,
        name: &str,
        alt_text: &str,
        image_url: &str,
        default_link_url: Option<&str>,
        folder: &str,
        tags: &[String],
    ) -> Result<MediaLibraryItem, Error> {
        validate_name(name)?;
        validate_text(alt_text, 500, "Alt text must be 500 characters or fewer.")?;
        let (folder, normalized_tags) = validate_folder_tags(folder, tags)?;
        let current_hash: Option<String> = self
            .connection()?
            .query_row("SELECT hash FROM media_library WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        let image_url = validate_url(image_url, current_hash.is_none())?.unwrap_or_default();
        let default_link_url = validate_url(default_link_url.unwrap_or_default(), false)?;
        self.connection()?
            .execute(
                "UPDATE media_library SET name=?2,alt_text=?3,image_url=?4,default_link_url=?5,folder=?6,tags_json=?7,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
                params![id, name.trim(), alt_text.trim(), image_url, default_link_url, folder, serde_json::to_string(&normalized_tags).map_err(storage)?],
            )
            .map_err(storage)?;
        self.media_library_item(id)
    }

    pub fn delete_media_library_item(&self, id: &str) -> Result<(), Error> {
        let hash: Option<String> = self
            .connection()?
            .query_row("SELECT hash FROM media_library WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        self.connection()?
            .execute("DELETE FROM media_library WHERE id=?1", [id])
            .map_err(storage)?;
        if let Some(hash) = hash {
            self.collect_content(std::slice::from_ref(&hash))?;
        }
        Ok(())
    }

    pub fn media_library_upload_bytes(&self, id: &str) -> Result<(String, Vec<u8>), Error> {
        let (hash, name): (String, String) = self
            .connection()?
            .query_row(
                "SELECT hash,name FROM media_library WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        let path = self.root.join("files").join(&hash);
        super::validate_texture(&path)?;
        let bytes = std::fs::read(path).map_err(storage)?;
        if bytes.len() > 40 * 1024 * 1024 || format!("{:x}", sha2::Sha256::digest(&bytes)) != hash {
            return Err(invalid(
                "The library image failed its integrity check or exceeds 40 MB.",
            ));
        }
        Ok((name, bytes))
    }

    pub fn set_media_library_remote(
        &self,
        id: &str,
        provider_image_id: &str,
        image_url: &str,
        owner_scope: &str,
        team_id: Option<&str>,
    ) -> Result<MediaLibraryItem, Error> {
        let image_url = validate_url(image_url, true)?.ok_or(Error::NotFound)?;
        if uuid::Uuid::parse_str(provider_image_id).is_err()
            || !["personal", "team"].contains(&owner_scope)
            || (owner_scope == "team") != team_id.is_some()
        {
            return Err(invalid(
                "The image host returned invalid sharing information.",
            ));
        }
        self.connection()?.execute(
            "UPDATE media_library SET provider_image_id=?2,image_url=?3,owner_scope=?4,team_id=?5,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
            params![id, provider_image_id, image_url, owner_scope, team_id],
        ).map_err(storage)?;
        self.media_library_item(id)
    }
}
