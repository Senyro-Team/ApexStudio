use super::{storage, Workspace};
use apex_core::{Error, MarkdownBlock, MarkdownBlockKind};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownPreset {
    pub id: String,
    pub name: String,
    pub block: MarkdownBlock,
    pub created_at: String,
    pub updated_at: String,
}

fn invalid(message: &str) -> Error {
    Error::InvalidContent(message.to_owned())
}

fn valid_https_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some())
}

fn read_preset(row: &rusqlite::Row<'_>) -> rusqlite::Result<MarkdownPreset> {
    let block_json: String = row.get(2)?;
    Ok(MarkdownPreset {
        id: row.get(0)?,
        name: row.get(1)?,
        block: serde_json::from_str(&block_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                block_json.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

impl Workspace {
    pub fn markdown_presets(&self) -> Result<Vec<MarkdownPreset>, Error> {
        let db = self.connection()?;
        let mut query = db
            .prepare("SELECT id,name,block_json,created_at,updated_at FROM markdown_presets ORDER BY name COLLATE NOCASE,id")
            .map_err(storage)?;
        let presets = query
            .query_map([], read_preset)
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        Ok(presets)
    }

    pub fn save_markdown_preset(
        &self,
        name: &str,
        block: &MarkdownBlock,
    ) -> Result<MarkdownPreset, Error> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
            return Err(invalid(
                "Reusable block names must contain 1–100 visible characters.",
            ));
        }
        let mut block = block.clone();
        block.id = uuid::Uuid::now_v7().to_string();
        block.media_hash = None;
        let kind = match &block.kind {
            MarkdownBlockKind::Text => "text",
            MarkdownBlockKind::Image => "image",
            MarkdownBlockKind::Invite => "invite",
        };
        match &block.kind {
            MarkdownBlockKind::Text
                if block.text.trim().is_empty() || block.text.chars().count() > 20_000 =>
            {
                return Err(invalid("Text blocks must contain up to 20,000 characters."));
            }
            MarkdownBlockKind::Image => {
                if !valid_https_url(&block.image_url)
                    || block.alt_text.chars().count() > 500
                    || (!block.link_url.is_empty() && !valid_https_url(&block.link_url))
                {
                    return Err(invalid(
                        "Image blocks need an HTTPS image URL and valid optional link.",
                    ));
                }
            }
            MarkdownBlockKind::Invite
                if !valid_https_url(&block.invite_url)
                    || block.text.trim().is_empty()
                    || block.text.chars().count() > 300 =>
            {
                return Err(invalid(
                    "Invite blocks need an HTTPS URL and a label under 300 characters.",
                ))
            }
            _ => {}
        }
        let id = uuid::Uuid::now_v7().to_string();
        let block_json = serde_json::to_string(&block).map_err(storage)?;
        self.connection()?
            .execute(
                "INSERT INTO markdown_presets(id,name,kind,block_json) VALUES(?1,?2,?3,?4)",
                params![id, name, kind, block_json],
            )
            .map_err(storage)?;
        self.markdown_preset(&id)
    }

    fn markdown_preset(&self, id: &str) -> Result<MarkdownPreset, Error> {
        self.connection()?
            .query_row(
                "SELECT id,name,block_json,created_at,updated_at FROM markdown_presets WHERE id=?1",
                [id],
                read_preset,
            )
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)
    }

    pub fn delete_markdown_preset(&self, id: &str) -> Result<(), Error> {
        let deleted = self
            .connection()?
            .execute("DELETE FROM markdown_presets WHERE id=?1", [id])
            .map_err(storage)?;
        if deleted == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }
}
