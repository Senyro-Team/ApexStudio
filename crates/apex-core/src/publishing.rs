use crate::Error;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishingInput {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub youtube_url: Option<String>,
    pub media: Vec<PublishingImage>,
    pub liveries: Vec<PublishingLivery>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishingImage {
    pub hash: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishingLivery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PublishingSource>,
    pub vehicle: String,
    pub primary_color: String,
    pub secondary_color: Option<String>,
    pub opacity: u8,
    pub sides: Vec<PublishingSide>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishingSource {
    pub folder_id: String,
    pub account_id: String,
    pub hashes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishingSide {
    pub side: String,
    pub decal_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishingDraft {
    pub data: PublishingInput,
    pub revision: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishingPreview {
    pub hash: String,
    pub name: String,
    pub size: u64,
    pub preview: Option<String>,
}
fn invalid(s: &str) -> Error {
    Error::InvalidContent(s.into())
}
impl PublishingInput {
    pub fn validate(&self, complete: bool) -> Result<(), Error> {
        if self.title.chars().count() > 50
            || self.description.chars().count() > 2000
            || self.tags.len() > 10
            || self
                .tags
                .iter()
                .any(|t| t.trim().is_empty() || t.chars().count() > 30)
        {
            return Err(invalid("Use a title up to 50 characters, a description up to 2000, and up to ten tags of 30 characters each."));
        }
        let mut tags = std::collections::HashSet::new();
        if self.tags.iter().any(|t| !tags.insert(t.to_lowercase())) {
            return Err(invalid("Remove duplicate tags."));
        }
        if let Some(link) = self.youtube_url.as_deref().filter(|l| !l.is_empty()) {
            let url = url::Url::parse(link).map_err(|_| invalid("Enter a valid YouTube URL."))?;
            if url.scheme() != "https"
                || ![
                    Some("youtube.com"),
                    Some("www.youtube.com"),
                    Some("youtu.be"),
                ]
                .contains(&url.host_str())
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(invalid("Enter an HTTPS YouTube URL."));
            }
        }
        if self.media.len() > 10 || self.liveries.len() > 20 {
            return Err(invalid("Use up to ten preview images and twenty liveries."));
        }
        let mut images = std::collections::HashSet::new();
        for image in &self.media {
            if image.hash.len() != 64
                || !image
                    .hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !images.insert(&image.hash)
                || image.name.is_empty()
                || image.name.len() > 255
                || image
                    .name
                    .chars()
                    .any(|c| c.is_control() || c == '/' || c == '\\')
            {
                return Err(invalid("Use distinct preview images with valid filenames."));
            }
        }
        for l in &self.liveries {
            if let Some(source) = &l.source {
                if source.folder_id.len() > 100
                    || source.account_id.len() > 20
                    || source.hashes.len() > 100
                    || source.hashes.iter().any(|h| !crate::valid_hash(h))
                {
                    return Err(invalid("Invalid linked livery source."));
                }
            }
            let color = |v: &str| {
                v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|b| b.is_ascii_hexdigit())
            };
            if l.vehicle.chars().count() > 120
                || !color(&l.primary_color)
                || l.secondary_color.as_deref().is_some_and(|c| !color(c))
                || l.opacity > 100
                || l.sides.len() > 100
            {
                return Err(invalid(
                    "Check vehicle name, colours, opacity (0-100) and side count.",
                ));
            }
            let mut sides = std::collections::HashSet::new();
            for side in &l.sides {
                if side.side.len() > 60
                    || side.side.chars().any(char::is_control)
                    || (!side.side.is_empty() && !sides.insert(&side.side))
                    || (!side.decal_id.is_empty()
                        && (side.decal_id.len() > 20
                            || side.decal_id.starts_with('0')
                            || !side.decal_id.bytes().all(|b| b.is_ascii_digit())))
                {
                    return Err(invalid(
                        "Use distinct side names and positive numeric decal IDs (up to 20 digits).",
                    ));
                }
                if complete && (side.side.trim().is_empty() || side.decal_id.is_empty()) {
                    return Err(invalid("Enter a side and decal ID for each code row."));
                }
            }
            if complete && (l.vehicle.trim().is_empty() || l.sides.is_empty()) {
                return Err(invalid(
                    "Each livery needs a vehicle and at least one side code.",
                ));
            }
        }
        if complete
            && (self.title.trim().chars().count() < 5
                || self.description.trim().is_empty()
                || self.media.len() < 2
                || self.liveries.is_empty())
        {
            return Err(invalid("Add a title of at least 5 characters, a description, 2-10 previews and at least one livery code."));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_drafts_are_saved_but_not_exportable() {
        let d = PublishingInput::default();
        assert!(d.validate(false).is_ok());
        assert!(d.validate(true).is_err());
    }
    #[test]
    fn rejects_unsafe_links_and_duplicate_media() {
        let mut d = PublishingInput {
            youtube_url: Some("javascript:alert(1)".into()),
            ..Default::default()
        };
        assert!(d.validate(false).is_err());
        d.youtube_url = None;
        d.media = vec![PublishingImage {
            hash: "a".repeat(64),
            name: "../private.png".into(),
        }];
        assert!(d.validate(false).is_err());
    }
}
