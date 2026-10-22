use crate::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Note {
    pub id: String,
    pub project_id: Option<String>,
    pub title: String,
    pub document: Value,
    pub markdown: String,
    pub revision: u32,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub fn validate_note(title: &str, document: &Value, markdown: &str) -> Result<(), Error> {
    let invalid = |s: &str| Error::InvalidContent(s.into());
    if title.trim().is_empty() || title.chars().count() > 120 || title.chars().any(char::is_control)
    {
        return Err(invalid("Enter a note title between 1 and 120 characters."));
    }
    if document.to_string().len() > 1_000_000 || markdown.len() > 1_000_000 {
        return Err(invalid("Keep each note under 1 MB."));
    }
    if document.get("type").and_then(Value::as_str) != Some("doc") {
        return Err(invalid("The note document is invalid."));
    }
    fn walk(value: &Value, depth: u32, count: &mut u32) -> Result<(), Error> {
        *count += 1;
        if depth > 32 || *count > 10_000 {
            return Err(Error::InvalidContent("This note is too complex.".into()));
        }
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        if ![
            "doc",
            "paragraph",
            "text",
            "heading",
            "bulletList",
            "orderedList",
            "listItem",
            "taskList",
            "taskItem",
            "blockquote",
            "codeBlock",
            "horizontalRule",
            "hardBreak",
            "image",
            "table",
            "tableRow",
            "tableCell",
            "tableHeader",
        ]
        .contains(&kind)
        {
            return Err(Error::InvalidContent(
                "The note contains an unsupported element.".into(),
            ));
        }
        if kind == "image" {
            let src = value
                .pointer("/attrs/src")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !src.strip_prefix("apex-image:").is_some_and(valid_hash) {
                return Err(Error::InvalidContent(
                    "Insert images using the local image picker.".into(),
                ));
            }
        }
        if let Some(marks) = value.get("marks").and_then(Value::as_array) {
            for mark in marks {
                let kind = mark.get("type").and_then(Value::as_str).unwrap_or("");
                if !["bold", "italic", "strike", "code", "link", "underline"].contains(&kind) {
                    return Err(Error::InvalidContent("Unsupported note formatting.".into()));
                }
                if kind == "link" {
                    let href = mark
                        .pointer("/attrs/href")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let url = url::Url::parse(href).map_err(|_| {
                        Error::InvalidContent("Enter a full HTTPS, HTTP or mailto link.".into())
                    })?;
                    if !["http", "https", "mailto"].contains(&url.scheme())
                        || !url.username().is_empty()
                        || url.password().is_some()
                    {
                        return Err(Error::InvalidContent("This link is not supported.".into()));
                    }
                }
            }
        }
        if let Some(children) = value.get("content").and_then(Value::as_array) {
            for child in children {
                walk(child, depth + 1, count)?;
            }
        }
        Ok(())
    }
    walk(document, 0, &mut 0)
}

pub fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn note_images(value: &Value) -> Vec<String> {
    let mut result = Vec::new();
    fn walk(value: &Value, result: &mut Vec<String>) {
        if let Some(src) = value
            .pointer("/attrs/src")
            .and_then(Value::as_str)
            .and_then(|s| s.strip_prefix("apex-image:"))
        {
            result.push(src.to_owned());
        }
        if let Some(children) = value.get("content").and_then(Value::as_array) {
            for child in children {
                walk(child, result);
            }
        }
    }
    walk(value, &mut result);
    result.sort();
    result.dedup();
    result
}
