use super::{projects::work, CommandError};
use apex_core::Error;
use apex_storage::{StatisticsPost, StatisticsSnapshot, Workspace};
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    page: u32,
    limit: u32,
    total: u32,
    total_pages: u32,
    items: Vec<StatisticsPost>,
}

fn page_is_consistent(page: &Page, number: u32, expected: Option<(u32, u32)>) -> bool {
    page.page == number
        && page.limit == 6
        && page.total_pages <= 500
        && page.total <= 3000
        && page.items.len() <= 6
        && (page.total_pages == page.total.div_ceil(6)
            || (page.total == 0 && page.total_pages == 1))
        && expected.is_none_or(|x| x == (page.total, page.total_pages))
}

#[tauri::command]
pub async fn statistics_profile(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<String>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.statistics_profile()).await
}

#[tauri::command]
pub async fn set_statistics_profile(
    profile_id: String,
    workspace: tauri::State<'_, Workspace>,
) -> Result<(), CommandError> {
    let ws = workspace.inner().clone();
    work(move || ws.set_statistics_profile(profile_id.trim())).await
}

#[tauri::command]
pub async fn statistics_history(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<StatisticsSnapshot>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || match ws.statistics_profile()? {
        Some(id) => ws.statistics_snapshots(&id),
        None => Ok(Vec::new()),
    })
    .await
}

#[tauri::command]
pub async fn statistics_sales(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Option<apex_storage::SalesSnapshot>, CommandError> {
    let ws = workspace.inner().clone();
    work(move || match ws.statistics_profile()? {
        Some(id) => ws.statistics_sales(&id),
        None => Ok(None),
    })
    .await
}

#[tauri::command]
pub async fn refresh_statistics(
    workspace: tauri::State<'_, Workspace>,
) -> Result<Vec<StatisticsSnapshot>, CommandError> {
    let ws = workspace.inner().clone();
    let read_ws = ws.clone();
    let id = work(move || {
        read_ws
            .statistics_profile()?
            .ok_or_else(|| Error::InvalidContent("Set a Roblox user ID in Settings first.".into()))
    })
    .await?;
    apex_storage::validate_statistics_profile(&id).map_err(CommandError::from)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| {
            CommandError::from(Error::InvalidContent(
                "Could not start the ERLCX connection.".into(),
            ))
        })?;
    let mut posts = Vec::new();
    let mut ids = HashSet::new();
    let mut page_number = 1u32;
    let mut expected = None;
    loop {
        let url = format!("https://api.erlcx.com/users/roblox/{id}/bundles");
        let response = client
            .get(url)
            .query(&[
                ("page", page_number.to_string()),
                ("limit", "6".into()),
                ("sortBy", "latest".into()),
            ])
            .send()
            .await
            .map_err(|_| {
                CommandError::from(Error::InvalidContent(
                    "Could not reach ERLCX. Try again later.".into(),
                ))
            })?;
        if !response.status().is_success() {
            return Err(Error::InvalidContent(format!(
                "ERLCX returned HTTP {}. Try again later.",
                response.status().as_u16()
            ))
            .into());
        }
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            CommandError::from(Error::InvalidContent(
                "The ERLCX response was interrupted.".into(),
            ))
        })? {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(
                    Error::InvalidContent("The ERLCX response was too large.".into()).into(),
                );
            }
            bytes.extend_from_slice(&chunk);
        }
        let page: Page = serde_json::from_slice(&bytes).map_err(|_| {
            CommandError::from(Error::InvalidContent(
                "ERLCX returned an unexpected response.".into(),
            ))
        })?;
        if !page_is_consistent(&page, page_number, expected) {
            return Err(Error::InvalidContent(
                "ERLCX changed the result while loading. Refresh again.".into(),
            )
            .into());
        }
        expected = Some((page.total, page.total_pages));
        for post in page.items {
            if post.bundle_id == 0
                || post.title.len() > 2000
                || !ids.insert(post.bundle_id)
                || post
                    .rating_average
                    .is_some_and(|v| !v.is_finite() || !(0.0..=5.0).contains(&v))
            {
                return Err(
                    Error::InvalidContent("ERLCX returned inconsistent post data.".into()).into(),
                );
            }
            posts.push(post);
        }
        if page_number >= page.total_pages {
            break;
        }
        page_number += 1;
    }
    if expected.is_some_and(|(count, _)| count as usize != posts.len()) {
        return Err(Error::InvalidContent(
            "ERLCX returned an incomplete post list. Refresh again.".into(),
        )
        .into());
    }
    work(move || {
        ws.save_statistics_snapshot(&id, &posts)?;
        ws.statistics_snapshots(&id)
    })
    .await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn validates_page_metadata_and_nullable_counters() {
        let page: Page = serde_json::from_str(r#"{"page":1,"limit":6,"total":1,"totalPages":1,"items":[{"bundleId":12,"title":"A","views":0,"likes":null}]}"#).unwrap();
        assert!(page_is_consistent(&page, 1, None));
        assert_eq!(page.items[0].views, Some(0));
        assert_eq!(page.items[0].likes, None);
        assert!(!page_is_consistent(&page, 2, None));
        assert!(!page_is_consistent(&page, 1, Some((2, 1))));
    }
}
