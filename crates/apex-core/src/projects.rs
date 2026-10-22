use crate::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectInput {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub vehicles: Vec<String>,
    pub reference_links: Vec<String>,
    pub deadline: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    #[serde(flatten)]
    pub fields: ProjectInput,
    pub created_at: String,
    pub updated_at: String,
    pub archived: bool,
    pub asset_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub hash: String,
    pub size: u64,
    pub imported_at: String,
    pub preview: Option<String>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: Vec<Asset>,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Vehicle {
    pub id: String,
    pub team: String,
    pub name: String,
    pub source: String,
    pub livery_support: String,
    pub body_status: Option<String>,
    pub window_status: Option<String>,
    pub notes: Vec<String>,
    pub slots: Vec<TemplateSlot>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateSlot {
    pub key: String,
    pub status: String,
    pub kind: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalogue {
    pub captured_at: String,
    pub source: String,
    pub completeness: String,
    pub license: String,
    pub vehicles: Vec<Vehicle>,
}

pub fn vehicle_catalogue() -> Result<Catalogue, Error> {
    serde_json::from_str(include_str!("../../../catalogue/official-vehicles.json"))
        .map_err(|error| Error::Storage(error.to_string()))
}

/// Validation is shared by every interface. Catalogue support is never inferred.
pub fn validate_project(mut input: ProjectInput) -> Result<ProjectInput, Error> {
    let invalid = |message: &str| Error::InvalidProject(message.to_owned());
    input.name = input.name.trim().to_owned();
    if input.name.is_empty()
        || input.name.chars().count() > 120
        || input.name.chars().any(char::is_control)
    {
        return Err(invalid(
            "Enter a project name between 1 and 120 characters.",
        ));
    }
    if input.description.chars().count() > 20000 {
        return Err(invalid("Keep the description under 20,000 characters."));
    }
    for (values, limit, length) in [
        (&mut input.tags, 30, 60),
        (&mut input.vehicles, 100, 180),
        (&mut input.reference_links, 50, 2048),
    ] {
        if values.len() > limit {
            return Err(invalid(
                "This project has too many tags, vehicles, or reference links.",
            ));
        }
        for value in values.iter_mut() {
            *value = value.trim().to_owned();
        }
        values.retain(|value| !value.is_empty());
        if values
            .iter()
            .any(|value| value.chars().count() > length || value.chars().any(char::is_control))
        {
            return Err(invalid(
                "A tag, vehicle, or reference link is too long or contains control characters.",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        values.retain(|value| seen.insert(value.to_lowercase()));
    }
    for link in &input.reference_links {
        let parsed = url::Url::parse(link)
            .map_err(|_| invalid("Reference links must be full http:// or https:// URLs."))?;
        if !["http", "https"].contains(&parsed.scheme())
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(invalid(
                "Reference links must be HTTP or HTTPS URLs without embedded credentials.",
            ));
        }
    }
    let catalogue = vehicle_catalogue()?;
    let legacy: serde_json::Value =
        serde_json::from_str(include_str!("../../../catalogue/vehicles.json"))
            .map_err(|e| Error::Storage(e.to_string()))?;
    if input.vehicles.iter().any(|name| {
        !catalogue
            .vehicles
            .iter()
            .any(|vehicle| vehicle.name == *name)
            && !legacy["vehicles"].as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|v| v["name"].as_str() == Some(name.as_str()))
            })
    }) {
        return Err(invalid("Select vehicles from the ER:LC catalogue."));
    }
    if let Some(deadline) = input.deadline.as_ref() {
        if deadline.is_empty() {
            input.deadline = None;
        } else if deadline.len() != 10
            || chrono::NaiveDate::parse_from_str(deadline, "%Y-%m-%d").is_err()
        {
            return Err(invalid("Choose a valid deadline date."));
        }
    }
    Ok(input)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_dates_links_and_unknown_vehicles() {
        for input in [
            ProjectInput {
                name: "Test".into(),
                deadline: Some("2026-02-30".into()),
                ..Default::default()
            },
            ProjectInput {
                name: "Test".into(),
                reference_links: vec!["javascript:alert(1)".into()],
                ..Default::default()
            },
            ProjectInput {
                name: "Test".into(),
                vehicles: vec!["Invented car".into()],
                ..Default::default()
            },
        ] {
            assert!(validate_project(input).is_err());
        }
    }
    #[test]
    fn trims_metadata_and_removes_duplicate_tags() {
        let fields = validate_project(ProjectInput {
            name: "  Project  ".into(),
            tags: vec!["Police".into(), " police ".into()],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(fields.name, "Project");
        assert_eq!(fields.tags, vec!["Police"]);
    }
}
