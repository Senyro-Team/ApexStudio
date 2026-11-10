use apex_core::Error;
use apex_storage::{StatisticsPost, StatisticsSnapshot, Workspace};
use reqwest::{redirect::Policy, Client};
use serde::Deserialize;
use std::collections::HashSet;
use std::time::Duration;

const PROFILE_API_BASE: &str = "https://api.erlcx.com";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    page: u32,
    limit: u32,
    total: u32,
    total_pages: u32,
    items: Vec<StatisticsPost>,
}

fn bad(message: &str) -> Error {
    Error::InvalidContent(message.into())
}

fn page_is_consistent(page: &Page, number: u32, expected: Option<(u32, u32)>) -> bool {
    page.page == number
        && page.limit == 6
        && page.total_pages <= 500
        && page.total <= 3000
        && page.items.len() <= 6
        && (page.total_pages == page.total.div_ceil(6)
            || (page.total == 0 && page.total_pages == 1))
        && expected.is_none_or(|value| value == (page.total, page.total_pages))
}

pub async fn refresh_public_profile(
    workspace: &Workspace,
) -> Result<Vec<StatisticsSnapshot>, Error> {
    refresh_public_profile_from(workspace, PROFILE_API_BASE).await
}

async fn refresh_public_profile_from(
    workspace: &Workspace,
    api_base: &str,
) -> Result<Vec<StatisticsSnapshot>, Error> {
    let profile_id = workspace
        .statistics_profile()?
        .ok_or_else(|| bad("Set a Roblox user ID in Settings first."))?;
    apex_storage::validate_statistics_profile(&profile_id)?;
    let client = Client::builder()
        .https_only(!cfg!(test))
        .timeout(Duration::from_secs(20))
        .redirect(Policy::none())
        .build()
        .map_err(|_| bad("Could not start the ERLCX connection."))?;
    let mut posts = Vec::new();
    let mut ids = HashSet::new();
    let mut page_number = 1u32;
    let mut expected = None;
    loop {
        let url = format!("{api_base}/users/roblox/{profile_id}/bundles");
        let response = client
            .get(url)
            .query(&[
                ("page", page_number.to_string()),
                ("limit", "6".into()),
                ("sortBy", "latest".into()),
            ])
            .send()
            .await
            .map_err(|_| bad("Could not reach ERLCX. Try again later."))?;
        if !response.status().is_success() {
            return Err(bad(&format!(
                "ERLCX returned HTTP {}. Try again later.",
                response.status().as_u16()
            )));
        }
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| bad("The ERLCX response was interrupted."))?
        {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(bad("The ERLCX response was too large."));
            }
            bytes.extend_from_slice(&chunk);
        }
        let page: Page = serde_json::from_slice(&bytes)
            .map_err(|_| bad("ERLCX returned an unexpected response."))?;
        if !page_is_consistent(&page, page_number, expected) {
            return Err(bad(
                "ERLCX changed the result while loading. Refresh again.",
            ));
        }
        expected = Some((page.total, page.total_pages));
        for post in page.items {
            if post.bundle_id == 0
                || post.title.len() > 2000
                || !ids.insert(post.bundle_id)
                || post
                    .rating_average
                    .is_some_and(|value| !value.is_finite() || !(0.0..=5.0).contains(&value))
            {
                return Err(bad("ERLCX returned inconsistent post data."));
            }
            posts.push(post);
        }
        if page_number >= page.total_pages {
            break;
        }
        page_number += 1;
    }
    if expected.is_some_and(|(count, _)| count as usize != posts.len()) {
        return Err(bad(
            "ERLCX returned an incomplete post list. Refresh again.",
        ));
    }
    workspace.save_statistics_snapshot(&profile_id, &posts)?;
    workspace.statistics_snapshots(&profile_id)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn page_metadata_and_nullable_counters_are_checked() {
        let page: Page = serde_json::from_str(
            r#"{"page":1,"limit":6,"total":1,"totalPages":1,"items":[{"bundleId":12,"title":"A","views":0,"likes":null}]}"#,
        )
        .expect("parse profile page fixture");
        assert!(page_is_consistent(&page, 1, None));
        assert_eq!(page.items[0].views, Some(0));
        assert_eq!(page.items[0].likes, None);
        assert!(!page_is_consistent(&page, 2, None));
        assert!(!page_is_consistent(&page, 1, Some((2, 1))));
    }

    #[test]
    fn public_refresh_reads_only_the_profile_bundles_endpoint_and_saves_a_snapshot() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("local mock server");
        let address = listener.local_addr().expect("mock server address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("profile request");
            let mut request = [0; 4096];
            let count = stream.read(&mut request).expect("read profile request");
            let request = String::from_utf8_lossy(&request[..count]);
            assert!(request.starts_with("GET /users/roblox/2919579206/bundles?"));
            let body = r#"{"page":1,"limit":6,"total":1,"totalPages":1,"items":[{"bundleId":123,"title":"Mock livery","views":42,"likes":7,"copies":2,"favourites":3,"ratingCount":4,"ratingAverage":4.5}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("mock profile response");
        });
        let directory = tempfile::tempdir().expect("temporary workspace directory");
        let workspace = Workspace::open(directory.path()).expect("dummy workspace");
        workspace
            .set_statistics_profile("2919579206")
            .expect("configure profile");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let history = runtime
            .block_on(refresh_public_profile_from(
                &workspace,
                &format!("http://{address}"),
            ))
            .expect("fetch profile snapshot from mock");
        server.join().expect("mock server completes");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].posts[0].bundle_id, 123);
        assert_eq!(history[0].posts[0].views, Some(42));
    }
}
