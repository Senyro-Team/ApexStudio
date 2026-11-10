use apex_core::Error;
use apex_storage::{
    RobloxGroupRevenueData, SalesSnapshot, StatisticsPost, StatisticsSnapshot, Workspace,
};
use reqwest::{redirect::Policy, Client};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsComparison {
    pub start_date: String,
    pub end_date: String,
    pub range_days: Option<i64>,
    pub erlcx_sales: u64,
    pub erlcx_net_robux: u64,
    pub erlcx_gross_robux_snapshot: u64,
    pub roblox_selected_groups: usize,
    pub roblox_selected_passes: usize,
    pub roblox_sale_rows: usize,
    pub roblox_price: u64,
    pub roblox_revenue: u64,
    pub roblox_coverage: String,
    pub daily: Vec<StatisticsComparisonDay>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsComparisonDay {
    pub date: String,
    pub erlcx_sales: u64,
    pub erlcx_net_robux: u64,
    pub roblox_price: Option<u64>,
    pub roblox_revenue: Option<u64>,
}

pub fn compare_statistics_sales(
    erlcx: &SalesSnapshot,
    roblox: &RobloxGroupRevenueData,
    range_days: Option<i64>,
) -> Result<StatisticsComparison, Error> {
    if range_days.is_some_and(|days| !(1..=3650).contains(&days)) {
        return Err(bad("Choose a comparison range from 1 to 3650 days."));
    }
    let erlcx_end = erlcx
        .data
        .series
        .iter()
        .filter_map(|point| parse_date(&point.date))
        .max()
        .ok_or_else(|| bad("ERLCX has no daily sales data to compare."))?;
    let range_start =
        range_days.map(|days| erlcx_end - chrono::Duration::days(days.saturating_sub(1)));
    let points = erlcx
        .data
        .series
        .iter()
        .filter_map(|point| {
            let date = parse_date(&point.date)?;
            range_start
                .is_none_or(|start| date >= start)
                .then_some((date.format("%Y-%m-%d").to_string(), point))
        })
        .collect::<Vec<_>>();
    let erlcx_start = points
        .iter()
        .map(|(date, _)| date.as_str())
        .min()
        .ok_or_else(|| bad("No ERLCX sales fall within this range."))?;
    let erlcx_end = points
        .iter()
        .map(|(date, _)| date.as_str())
        .max()
        .ok_or_else(|| bad("No ERLCX sales fall within this range."))?;

    let groups = roblox
        .groups
        .iter()
        .filter(|group| !group.selected_pass_ids.is_empty())
        .collect::<Vec<_>>();
    if groups.is_empty() {
        return Err(bad(
            "Select Roblox passes in Group Revenue before comparing sales.",
        ));
    }
    let selected_group_ids = groups
        .iter()
        .map(|group| group.group_id.as_str())
        .collect::<HashSet<_>>();
    let selected_assets = groups
        .iter()
        .flat_map(|group| {
            group
                .selected_pass_ids
                .iter()
                .map(move |asset_id| (group.group_id.as_str(), asset_id.as_str()))
        })
        .collect::<HashSet<_>>();
    let sales = roblox
        .sales
        .iter()
        .filter_map(|sale| {
            let date = parse_date(&sale.sale_at)?;
            (selected_group_ids.contains(sale.group_id.as_str())
                && selected_assets.contains(&(sale.group_id.as_str(), sale.asset_id.as_str())))
            .then_some((date.format("%Y-%m-%d").to_string(), sale))
        })
        .collect::<Vec<_>>();
    let observed_start = sales.iter().map(|(date, _)| date.as_str()).min();
    let observed_end = sales.iter().map(|(date, _)| date.as_str()).max();
    let complete_coverage = groups.iter().all(|group| {
        group
            .api_coverage_from
            .as_deref()
            .and_then(parse_date)
            .is_some()
            && group
                .api_coverage_to
                .as_deref()
                .and_then(parse_date)
                .is_some()
    });
    let roblox_start = if complete_coverage {
        groups
            .iter()
            .filter_map(|group| group.api_coverage_from.as_deref().and_then(parse_date))
            .map(|date| date.format("%Y-%m-%d").to_string())
            .max()
    } else {
        observed_start.map(str::to_owned)
    }
    .ok_or_else(|| bad("No dated Roblox sales are available."))?;
    let roblox_end = if complete_coverage {
        groups
            .iter()
            .filter_map(|group| group.api_coverage_to.as_deref().and_then(parse_date))
            .map(|date| date.format("%Y-%m-%d").to_string())
            .min()
    } else {
        observed_end.map(str::to_owned)
    }
    .ok_or_else(|| bad("No Roblox and ERLCX coverage overlap."))?;
    let start_date = std::cmp::max(erlcx_start.to_owned(), roblox_start);
    let end_date = std::cmp::min(erlcx_end.to_owned(), roblox_end);
    if start_date > end_date {
        return Err(bad("No Roblox and ERLCX coverage overlap."));
    }
    let points = points
        .into_iter()
        .filter(|(date, _)| date >= &start_date && date <= &end_date)
        .collect::<Vec<_>>();
    if points.is_empty() {
        return Err(bad(
            "No ERLCX daily points fall within the shared coverage window.",
        ));
    }
    let matching_sales = sales
        .iter()
        .filter(|(date, _)| date >= &start_date && date <= &end_date)
        .collect::<Vec<_>>();
    let mut daily = points
        .iter()
        .map(|(date, point)| {
            (
                date.clone(),
                StatisticsComparisonDay {
                    date: date.clone(),
                    erlcx_sales: point.sales,
                    erlcx_net_robux: point.net_robux,
                    roblox_price: complete_coverage.then_some(0),
                    roblox_revenue: complete_coverage.then_some(0),
                },
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    for (date, sale) in &matching_sales {
        if let Some(day) = daily.get_mut(date) {
            day.roblox_price = Some(day.roblox_price.unwrap_or(0).saturating_add(sale.price));
            day.roblox_revenue = Some(day.roblox_revenue.unwrap_or(0).saturating_add(sale.revenue));
        }
    }
    let daily = daily.into_values().collect::<Vec<_>>();
    let roblox_price = matching_sales
        .iter()
        .map(|(_, sale)| sale.price)
        .fold(0, u64::saturating_add);
    let roblox_revenue = matching_sales
        .iter()
        .map(|(_, sale)| sale.revenue)
        .fold(0, u64::saturating_add);
    let erlcx_sales = points
        .iter()
        .map(|(_, point)| point.sales)
        .fold(0, u64::saturating_add);
    let erlcx_net_robux = points
        .iter()
        .map(|(_, point)| point.net_robux)
        .fold(0, u64::saturating_add);
    Ok(StatisticsComparison {
        start_date,
        end_date,
        range_days,
        erlcx_sales,
        erlcx_net_robux,
        erlcx_gross_robux_snapshot: erlcx.data.gross_robux,
        roblox_selected_groups: groups.len(),
        roblox_selected_passes: selected_assets.len(),
        roblox_sale_rows: matching_sales.len(),
        roblox_price,
        roblox_revenue,
        roblox_coverage: if complete_coverage {
            "complete_api".into()
        } else {
            "observed_rows_only".into()
        },
        daily,
    })
}

fn parse_date(value: &str) -> Option<chrono::NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&chrono::Utc).date_naive())
        .or_else(|| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
}

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

    #[test]
    fn comparison_uses_only_overlapping_dates_and_marks_partial_coverage_unknown() {
        let erlcx = SalesSnapshot {
            collected_at: "2026-10-26T23:00:00Z".into(),
            data: apex_storage::SalesData {
                sales: 5,
                all_time_sales: 100,
                net_robux: 50,
                gross_robux: 150,
                series: vec![
                    apex_storage::SalesPoint {
                        date: "2026-10-24".into(),
                        sales: 2,
                        net_robux: 20,
                    },
                    apex_storage::SalesPoint {
                        date: "2026-10-25".into(),
                        sales: 3,
                        net_robux: 30,
                    },
                    apex_storage::SalesPoint {
                        date: "2026-10-26".into(),
                        sales: 2,
                        net_robux: 40,
                    },
                ],
            },
        };
        let group = apex_storage::RobloxRevenueGroup {
            group_id: "group-1".into(),
            name: "Apex".into(),
            experience_ids: Vec::new(),
            pass_mappings: Default::default(),
            selected_pass_ids: vec!["pass-1".into()],
            api_updated_at: None,
            api_coverage_from: Some("2026-10-24".into()),
            api_coverage_to: Some("2026-10-26".into()),
            api_summary: None,
            api_status: Some("ok".into()),
            updated_at: None,
            last_import_at: None,
        };
        let sale = apex_storage::RobloxGroupSale {
            group_id: "group-1".into(),
            sale_fingerprint: "fingerprint".into(),
            universe_id: "universe-1".into(),
            asset_id: "pass-1".into(),
            sale_at: "2026-10-25T01:00:00+14:00".into(),
            sale_location: "Game".into(),
            hold_status: "released".into(),
            price: 100,
            revenue: 70,
            import_batch_id: "batch-1".into(),
            external_transaction_id: None,
            source: "api".into(),
            dedupe_confidence: "exact_fields".into(),
            imported_at: None,
        };
        let mut roblox = RobloxGroupRevenueData {
            groups: vec![group],
            sales: vec![
                sale,
                apex_storage::RobloxGroupSale {
                    group_id: "group-1".into(),
                    sale_fingerprint: "fingerprint-2".into(),
                    universe_id: "universe-1".into(),
                    asset_id: "pass-1".into(),
                    sale_at: "2026-10-26T18:00:00Z".into(),
                    sale_location: "Game".into(),
                    hold_status: "released".into(),
                    price: 50,
                    revenue: 35,
                    import_batch_id: "batch-1".into(),
                    external_transaction_id: None,
                    source: "api".into(),
                    dedupe_confidence: "exact_fields".into(),
                    imported_at: None,
                },
            ],
            passes: Vec::new(),
        };
        let complete = compare_statistics_sales(&erlcx, &roblox, Some(30))
            .expect("overlapping API coverage compares");
        assert!(compare_statistics_sales(&erlcx, &roblox, Some(-1)).is_err());
        let one_day =
            compare_statistics_sales(&erlcx, &roblox, Some(1)).expect("one-day range compares");
        assert_eq!(one_day.start_date, "2026-10-26");
        assert_eq!(one_day.daily.len(), 1);
        assert_eq!(complete.start_date, "2026-10-24");
        assert_eq!(complete.end_date, "2026-10-26");
        assert_eq!(complete.roblox_coverage, "complete_api");
        assert_eq!(complete.roblox_price, 150);
        assert_eq!(complete.daily[0].roblox_price, Some(100));
        assert_eq!(complete.daily[1].roblox_price, Some(0));
        assert_eq!(complete.daily[2].roblox_price, Some(50));

        roblox.groups[0].api_coverage_from = None;
        roblox.groups[0].api_coverage_to = None;
        let partial = compare_statistics_sales(&erlcx, &roblox, Some(30))
            .expect("observed CSV/API rows can still be compared");
        assert_eq!(partial.roblox_coverage, "observed_rows_only");
        assert_eq!(partial.daily[0].roblox_price, Some(100));
        assert_eq!(partial.daily[1].roblox_price, None);
    }
}
