use super::{storage, Workspace};
use apex_core::Error;
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
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
    pub owner_scope: String,
    pub created_at: String,
    pub updated_at: String,
    pub preview: Option<String>,
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
                "SELECT id,hash,name,alt_text,image_url,default_link_url,owner_scope,created_at,updated_at FROM media_library WHERE id=?1",
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
            owner_scope: row.6,
            created_at: row.7,
            updated_at: row.8,
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

    fn insert_media_item(
        &self,
        hash: Option<&str>,
        name: &str,
        alt_text: &str,
        image_url: &str,
        default_link_url: Option<&str>,
    ) -> Result<MediaLibraryItem, Error> {
        validate_name(name)?;
        validate_text(alt_text, 500, "Alt text must be 500 characters or fewer.")?;
        let image_url = validate_url(image_url, hash.is_none())?.unwrap_or_default();
        let default_link_url = validate_url(default_link_url.unwrap_or_default(), false)?;
        let id = uuid::Uuid::now_v7().to_string();
        self.connection()?
            .execute(
                "INSERT INTO media_library(id,hash,name,alt_text,image_url,default_link_url) VALUES(?1,?2,?3,?4,?5,?6)",
                params![id, hash, name.trim(), alt_text.trim(), image_url, default_link_url],
            )
            .map_err(storage)?;
        self.media_library_item(&id)
    }

    fn existing_hash_item(&self, hash: &str) -> Result<Option<MediaLibraryItem>, Error> {
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
        self.insert_media_item(Some(&hash), &name, &alt, "", None)
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
        self.insert_media_item(Some(&hash), &name, &alt, "", None)
    }

    pub fn add_media_library_url(
        &self,
        name: &str,
        alt_text: &str,
        image_url: &str,
        default_link_url: Option<&str>,
    ) -> Result<MediaLibraryItem, Error> {
        self.insert_media_item(None, name, alt_text, image_url, default_link_url)
    }

    pub fn update_media_library_item(
        &self,
        id: &str,
        name: &str,
        alt_text: &str,
        image_url: &str,
        default_link_url: Option<&str>,
    ) -> Result<MediaLibraryItem, Error> {
        validate_name(name)?;
        validate_text(alt_text, 500, "Alt text must be 500 characters or fewer.")?;
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
                "UPDATE media_library SET name=?2,alt_text=?3,image_url=?4,default_link_url=?5,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
                params![id, name.trim(), alt_text.trim(), image_url, default_link_url],
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
}
