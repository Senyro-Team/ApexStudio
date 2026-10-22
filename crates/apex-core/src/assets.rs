use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetFolder {
    pub id: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub vehicle_id: Option<String>,
    pub cover_hash: Option<String>,
    #[serde(default)]
    pub preview: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Placement {
    pub asset_id: String,
    pub folder_id: Option<String>,
    pub slot: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetVersion {
    pub id: String,
    pub asset_id: String,
    pub hash: String,
    pub name: String,
    pub created_at: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBoard {
    pub assets: Vec<crate::Asset>,
    pub folders: Vec<AssetFolder>,
    pub placements: Vec<Placement>,
}

/// Filename parsing is conservative: complete tokens, explicit window prefix, first following number.
pub fn suggest_slot(filename: &str) -> Option<String> {
    let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
    let mut expanded = String::new();
    let mut previous = None;
    for c in stem.chars() {
        if previous.is_some_and(|p: char| p.is_ascii_digit() != c.is_ascii_digit())
            && c.is_ascii_alphanumeric()
        {
            expanded.push(' ');
        }
        if c.is_ascii_alphanumeric() {
            expanded.push(c.to_ascii_lowercase());
        } else {
            expanded.push(' ');
        }
        previous = Some(c);
    }
    let tokens: Vec<_> = expanded.split_whitespace().collect();
    let mut found = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let mut word = *token;
        let mut window = i > 0 && ["window", "windows", "win", "wnd"].contains(&tokens[i - 1]);
        for prefix in ["windows", "window", "win", "wnd"] {
            if let Some(rest) = word.strip_prefix(prefix) {
                if !rest.is_empty() {
                    window = true;
                    word = rest;
                    break;
                }
            }
        }
        let side = match word {
            "left" | "lft" | "lf" => "Left",
            "right" | "rgt" | "rght" | "rt" => "Right",
            "front" | "frt" | "frnt" => "Front",
            "back" | "rear" | "bk" => "Back",
            "top" | "roof" => "Top",
            _ => continue,
        };
        let n = tokens
            .iter()
            .skip(i + 1)
            .find(|v| v.chars().all(|c| c.is_ascii_digit()))
            .and_then(|v| v.parse::<u32>().ok())?;
        if n == 0 || n > 100 {
            return None;
        }
        found.push(format!("{}{side} {n}", if window { "Window " } else { "" }));
    }
    if found.len() == 1 {
        found.pop()
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_use_first_number_after_side() {
        for (name, want) in [
            ("Patrol LEFT 2 7.png", "Left 2"),
            ("Patrol Lft2_7.png", "Left 2"),
            ("Unit 2024 Rear 1 9.png", "Back 1"),
            ("Patrol Window Left 2 7.png", "Window Left 2"),
            ("WinRight3_42.png", "Window Right 3"),
            ("frontier_RGT_1.png", "Right 1"),
        ] {
            assert_eq!(suggest_slot(name).as_deref(), Some(want));
        }
        for name in ["left.png", "left1right2.png", "bright1.png", "left0.png"] {
            assert!(suggest_slot(name).is_none(), "{name}");
        }
    }
}
