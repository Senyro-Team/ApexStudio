use super::{storage, Workspace};
use apex_core::Error;
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::io::Cursor;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayTemplate {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub layers_json: String,
    pub created_at: String,
    pub updated_at: String,
}

fn invalid(message: &str) -> Error {
    Error::InvalidContent(message.into())
}

impl Workspace {
    pub fn overlay_image_data(&self, hash: &str, width: u32, height: u32) -> Result<String, Error> {
        if hash.len() != 64
            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            || width == 0
            || height == 0
            || width > 1920
            || height > 1200
        {
            return Err(invalid(
                "Choose a managed image and a supported canvas size.",
            ));
        }
        let path = self.root.join("files").join(hash);
        let bytes = std::fs::read(path).map_err(storage)?;
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|_| invalid("The selected managed image is not a readable PNG."))?;
        let resized = image.thumbnail(width, height);
        let mut png = Cursor::new(Vec::new());
        resized
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(storage)?;
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png.get_ref())
        ))
    }

    fn overlay_template(&self, id: &str) -> Result<OverlayTemplate, Error> {
        self.connection()?
            .query_row(
                "SELECT id,name,width,height,layers_json,created_at,updated_at FROM overlay_templates WHERE id=?1",
                [id],
                |row| Ok(OverlayTemplate {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    width: row.get(2)?,
                    height: row.get(3)?,
                    layers_json: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                }),
            )
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)
    }

    pub fn overlay_templates(&self) -> Result<Vec<OverlayTemplate>, Error> {
        let db = self.connection()?;
        let mut statement = db
            .prepare("SELECT id,name,width,height,layers_json,created_at,updated_at FROM overlay_templates ORDER BY updated_at DESC,name COLLATE NOCASE")
            .map_err(storage)?;
        let templates = statement
            .query_map([], |row| {
                Ok(OverlayTemplate {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    width: row.get(2)?,
                    height: row.get(3)?,
                    layers_json: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        Ok(templates)
    }

    pub fn save_overlay_template(
        &self,
        id: Option<&str>,
        name: &str,
        width: u32,
        height: u32,
        layers_json: &str,
    ) -> Result<OverlayTemplate, Error> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
            return Err(invalid(
                "Template names must contain 1–120 visible characters.",
            ));
        }
        if width != 1920 || ![1080, 1200].contains(&height) {
            return Err(invalid(
                "Choose the ERLCX 16:9 or ExecLab 16:10 canvas size.",
            ));
        }
        if layers_json.len() > 8 * 1024 * 1024 {
            return Err(invalid(
                "This overlay template exceeds the 8 MB workspace limit.",
            ));
        }
        let layers: serde_json::Value = serde_json::from_str(layers_json)
            .map_err(|_| invalid("Overlay template data is invalid."))?;
        let layers = layers
            .as_array()
            .filter(|layers| layers.len() <= 50)
            .ok_or_else(|| invalid("Overlay templates can contain up to 50 layers."))?;
        for layer in layers {
            let kind = layer.get("kind").and_then(serde_json::Value::as_str);
            if !matches!(kind, Some("text" | "image" | "shape")) {
                return Err(invalid("Overlay template contains an unsupported layer."));
            }
            for (field, min, max) in [
                ("x", -3840.0, 3840.0),
                ("y", -2400.0, 2400.0),
                ("width", 1.0, 3840.0),
                ("height", 1.0, 2400.0),
                ("opacity", 0.0, 100.0),
                ("rotation", -180.0, 180.0),
            ] {
                let value = layer.get(field).and_then(serde_json::Value::as_f64);
                if !value.is_some_and(|value| value.is_finite() && value >= min && value <= max) {
                    return Err(invalid(
                        "Overlay layer geometry is outside supported bounds.",
                    ));
                }
            }
            if layer
                .get("name")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|name| name.chars().count() > 120)
            {
                return Err(invalid(
                    "Overlay layer names must be 120 characters or fewer.",
                ));
            }
            if kind == Some("text")
                && layer
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|text| text.chars().count() > 5000)
            {
                return Err(invalid("Overlay text must be 5,000 characters or fewer."));
            }
            if let Some(src) = layer.get("src").and_then(serde_json::Value::as_str) {
                if src.len() > 3 * 1024 * 1024
                    || !src.starts_with("data:image/")
                    || !src.contains(";base64,")
                {
                    return Err(invalid(
                        "Overlay images must be embedded local image files up to 3 MB each.",
                    ));
                }
            }
        }
        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        let exists: bool = self
            .connection()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM overlay_templates WHERE id=?1)",
                [&id],
                |row| row.get(0),
            )
            .map_err(storage)?;
        if exists {
            self.connection()?
                .execute(
                    "UPDATE overlay_templates SET name=?2,width=?3,height=?4,layers_json=?5,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
                    params![id, name, width, height, layers_json],
                )
                .map_err(storage)?;
        } else {
            self.connection()?
                .execute(
                    "INSERT INTO overlay_templates(id,name,width,height,layers_json) VALUES(?1,?2,?3,?4,?5)",
                    params![id, name, width, height, layers_json],
                )
                .map_err(storage)?;
        }
        self.overlay_template(&id)
    }

    pub fn delete_overlay_template(&self, id: &str) -> Result<(), Error> {
        let count = self
            .connection()?
            .execute("DELETE FROM overlay_templates WHERE id=?1", [id])
            .map_err(storage)?;
        if count == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }
}
