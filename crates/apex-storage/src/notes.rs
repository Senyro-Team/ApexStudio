use super::{storage, Workspace};
use apex_core::{Error, Note};
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use std::{fs, path::Path};

impl Workspace {
    pub fn notes(&self, project_id: Option<&str>) -> Result<Vec<Note>, Error> {
        let connection = self.connection()?;
        let mut query = connection.prepare("SELECT id,project_id,title,document,markdown,revision,archived,created_at,updated_at FROM notes WHERE project_id IS ?1 ORDER BY updated_at DESC,id DESC").map_err(storage)?;
        let rows = query
            .query_map([project_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get::<_, String>(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            })
            .map_err(storage)?;
        rows.map(|row| {
            let (
                id,
                project_id,
                title,
                document,
                markdown,
                revision,
                archived,
                created_at,
                updated_at,
            ) = row.map_err(storage)?;
            Ok(Note {
                id,
                project_id,
                title,
                document: serde_json::from_str(&document).map_err(storage)?,
                markdown,
                revision,
                archived,
                created_at,
                updated_at,
            })
        })
        .collect()
    }
    pub fn note(&self, id: &str) -> Result<Note, Error> {
        let project_id: Option<Option<String>> = self
            .connection()?
            .query_row("SELECT project_id FROM notes WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(storage)?;
        self.notes(project_id.ok_or(Error::NotFound)?.as_deref())?
            .into_iter()
            .find(|n| n.id == id)
            .ok_or(Error::NotFound)
    }
    pub fn create_note(&self, project_id: Option<&str>) -> Result<Note, Error> {
        if let Some(id) = project_id {
            if self.project(id)?.archived {
                return Err(Error::Archived);
            }
        }
        let id = uuid::Uuid::now_v7().to_string();
        self.connection()?.execute("INSERT INTO notes(id,project_id,title,document,markdown) VALUES(?1,?2,'Untitled note',?3,'')",params![id,project_id,r#"{"type":"doc","content":[{"type":"paragraph"}]}"#]).map_err(storage)?;
        self.note(&id)
    }
    pub fn save_note(
        &self,
        id: &str,
        revision: u32,
        title: &str,
        document: &serde_json::Value,
        markdown: &str,
    ) -> Result<Note, Error> {
        apex_core::validate_note(title, document, markdown)?;
        let note = self.note(id)?;
        if let Some(project) = note.project_id.as_deref() {
            if self.project(project)?.archived {
                return Err(Error::Archived);
            }
        }
        if note.archived {
            return Err(Error::InvalidContent(
                "Restore this note before editing.".into(),
            ));
        }
        for hash in apex_core::note_images(document) {
            let exists: bool = self
                .connection()?
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM note_images WHERE note_id=?1 AND hash=?2)",
                    params![id, hash],
                    |r| r.get(0),
                )
                .map_err(storage)?;
            if !exists {
                return Err(Error::InvalidContent(
                    "The note image is missing. Insert it again.".into(),
                ));
            }
        }
        let changed=self.connection()?.execute("UPDATE notes SET title=?1,document=?2,markdown=?3,revision=revision+1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?4 AND revision=?5 AND archived=0",params![title.trim(),document.to_string(),markdown,id,revision]).map_err(storage)?;
        if changed == 0 {
            return Err(Error::Conflict);
        }
        self.note(id)
    }
    pub fn archive_note(&self, id: &str, revision: u32, archived: bool) -> Result<Note, Error> {
        let note = self.note(id)?;
        if let Some(project) = note.project_id.as_deref() {
            if self.project(project)?.archived {
                return Err(Error::Archived);
            }
        }
        if self.connection()?.execute("UPDATE notes SET archived=?1,revision=revision+1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2 AND revision=?3",params![archived,id,revision]).map_err(storage)?==0 { return Err(Error::Conflict); }
        self.note(id)
    }
    pub fn attach_note_image(&self, id: &str, path: &Path) -> Result<String, Error> {
        let note = self.note(id)?;
        if note.archived {
            return Err(Error::InvalidContent(
                "Restore this note before editing.".into(),
            ));
        }
        if let Some(project) = note.project_id.as_deref() {
            if self.project(project)?.archived {
                return Err(Error::Archived);
            }
        }
        if fs::metadata(path).map_err(storage)?.len() > 8 * 1024 * 1024 {
            return Err(Error::InvalidContent(
                "Choose a PNG image smaller than 8 MB.".into(),
            ));
        }
        let bytes = fs::read(path).map_err(storage)?;
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(80 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().map_err(|_| {
            Error::InvalidContent("Choose a valid PNG image up to 4096 × 4096 pixels.".into())
        })?;
        let hash = self.import_file(path)?;
        self.connection()?
            .execute(
                "INSERT OR IGNORE INTO note_images(note_id,hash) VALUES(?1,?2)",
                params![id, hash],
            )
            .map_err(storage)?;
        Ok(hash)
    }
    pub fn note_image(&self, id: &str, hash: &str) -> Result<String, Error> {
        if !apex_core::valid_hash(hash) {
            return Err(Error::NotFound);
        }
        let exists: bool = self
            .connection()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM note_images WHERE note_id=?1 AND hash=?2)",
                params![id, hash],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if !exists {
            return Err(Error::NotFound);
        }
        let path = self.root.join("files").join(hash);
        if fs::metadata(&path).map_err(storage)?.len() > 8 * 1024 * 1024 {
            return Err(Error::InvalidContent(
                "Image exceeds the preview limit.".into(),
            ));
        }
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(fs::read(path).map_err(storage)?)
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn notes_reopen_and_conflicting_saves_do_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path()).unwrap();
        let n = ws.create_note(None).unwrap();
        let doc = serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Private notes"}]}]});
        let saved = ws
            .save_note(&n.id, n.revision, "Ideas", &doc, "Private notes")
            .unwrap();
        assert!(matches!(
            ws.save_note(&n.id, n.revision, "Old", &doc, "Old"),
            Err(Error::Conflict)
        ));
        let reopened = Workspace::open(dir.path()).unwrap().note(&n.id).unwrap();
        assert_eq!(reopened.document, doc);
        assert_eq!(reopened.title, "Ideas");
        ws.archive_note(&saved.id, saved.revision, true).unwrap();
        assert!(ws.note(&n.id).unwrap().archived);
    }
    #[test]
    fn rejects_remote_images_and_script_links() {
        for document in [
            serde_json::json!({"type":"doc","content":[{"type":"image","attrs":{"src":"https://tracker.invalid/x.png"}}]}),
            serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"x","marks":[{"type":"link","attrs":{"href":"javascript:alert(1)"}}]}]}]}),
        ] {
            assert!(apex_core::validate_note("Title", &document, "").is_err());
        }
    }
}
