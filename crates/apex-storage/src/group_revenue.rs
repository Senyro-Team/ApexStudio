use super::{storage, Error, Workspace};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RobloxRevenueGroup {
    pub group_id: String,
    pub name: String,
    pub experience_ids: Vec<String>,
    pub pass_mappings: BTreeMap<String, String>,
    #[serde(default)]
    pub selected_pass_ids: Vec<String>,
    #[serde(default)]
    pub api_updated_at: Option<String>,
    #[serde(default)]
    pub api_coverage_from: Option<String>,
    #[serde(default)]
    pub api_coverage_to: Option<String>,
    #[serde(default)]
    pub api_summary: Option<serde_json::Value>,
    #[serde(default)]
    pub api_status: Option<String>,
    pub updated_at: Option<String>,
    #[serde(skip_deserializing)]
    pub last_import_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RobloxGroupSale {
    #[serde(skip_deserializing)]
    pub group_id: String,
    pub sale_fingerprint: String,
    pub universe_id: String,
    pub asset_id: String,
    pub sale_at: String,
    pub sale_location: String,
    pub hold_status: String,
    pub price: u64,
    pub revenue: u64,
    pub import_batch_id: String,
    #[serde(default)]
    pub external_transaction_id: Option<String>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default = "default_confidence")]
    pub dedupe_confidence: String,
    #[serde(skip_deserializing)]
    pub imported_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RobloxGroupRevenueData {
    pub groups: Vec<RobloxRevenueGroup>,
    pub sales: Vec<RobloxGroupSale>,
    pub passes: Vec<RobloxRevenuePass>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RobloxRevenuePass {
    pub group_id: String,
    pub asset_id: String,
    pub universe_id: String,
    pub name: String,
    pub source: String,
}

fn default_source() -> String {
    "csv".into()
}
fn default_confidence() -> String {
    "exact_fields".into()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RobloxGroupRevenueImportResult {
    pub inserted: usize,
    pub duplicates: usize,
}

fn valid_numeric_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 20
        && id.bytes().all(|byte| byte.is_ascii_digit())
        && id.bytes().any(|byte| byte != b'0')
}

fn validate_group(group: &RobloxRevenueGroup) -> Result<(), Error> {
    if !valid_numeric_id(&group.group_id)
        || group.name.trim().is_empty()
        || group.name.chars().count() > 120
        || group.experience_ids.len() > 100
        || group.experience_ids.iter().any(|id| !valid_numeric_id(id))
    {
        return Err(Error::InvalidContent(
            "Enter a group name, numeric group ID, and one or more valid experience IDs.".into(),
        ));
    }
    let mut experiences = HashSet::new();
    if group
        .experience_ids
        .iter()
        .any(|id| !experiences.insert(id))
    {
        return Err(Error::InvalidContent(
            "Experience IDs must be unique.".into(),
        ));
    }
    if group.selected_pass_ids.len() > 1000
        || group
            .selected_pass_ids
            .iter()
            .any(|id| !valid_numeric_id(id))
        || group.pass_mappings.len() > 1000
        || group.pass_mappings.iter().any(|(id, name)| {
            !valid_numeric_id(id) || name.trim().is_empty() || name.chars().count() > 200
        })
    {
        return Err(Error::InvalidContent(
            "Game Pass mappings must use valid asset IDs and non-empty names up to 200 characters."
                .into(),
        ));
    }
    Ok(())
}

fn validate_sale(sale: &RobloxGroupSale, group: &RobloxRevenueGroup) -> Result<(), Error> {
    let date_prefix = sale.sale_at.get(0..10).unwrap_or_default();
    let valid_date = date_prefix.len() == 10
        && date_prefix.bytes().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        });
    if sale.sale_fingerprint.len() != 64
        || !sale
            .sale_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || !valid_numeric_id(&sale.universe_id)
        || !valid_numeric_id(&sale.asset_id)
        || !group
            .experience_ids
            .iter()
            .any(|id| id == &sale.universe_id)
        || sale.sale_at.len() > 40
        || !valid_date
        || sale.sale_location.len() > 200
        || !matches!(sale.hold_status.as_str(), "Released" | "Held" | "Cancelled")
        || sale.price > i64::MAX as u64
        || sale.revenue > i64::MAX as u64
        || sale.import_batch_id.is_empty()
        || sale.import_batch_id.len() > 100
        || !matches!(sale.source.as_str(), "csv" | "api")
        || !matches!(
            sale.dedupe_confidence.as_str(),
            "transaction_id" | "exact_fields" | "ambiguous"
        )
        || sale
            .external_transaction_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 200)
    {
        return Err(Error::InvalidContent(
            "The imported Roblox sales row is invalid.".into(),
        ));
    }
    Ok(())
}

impl Workspace {
    pub fn roblox_group_revenue(&self) -> Result<RobloxGroupRevenueData, Error> {
        let conn = self.connection()?;
        let mut groups = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT group_id,name,experience_ids_json,pass_mappings_json,updated_at,last_import_at,selected_pass_ids_json,api_updated_at,api_coverage_from,api_coverage_to,api_summary_json,api_status FROM roblox_group_revenue_groups ORDER BY name COLLATE NOCASE,group_id").map_err(storage)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, String>(11)?,
                    ))
                })
                .map_err(storage)?;
            for row in rows {
                let (
                    group_id,
                    name,
                    experience_ids,
                    pass_mappings,
                    updated_at,
                    last_import_at,
                    selected_pass_ids,
                    api_updated_at,
                    api_coverage_from,
                    api_coverage_to,
                    api_summary,
                    api_status,
                ) = row.map_err(storage)?;
                groups.push(RobloxRevenueGroup {
                    group_id,
                    name,
                    experience_ids: serde_json::from_str(&experience_ids).map_err(storage)?,
                    pass_mappings: serde_json::from_str(&pass_mappings).map_err(storage)?,
                    selected_pass_ids: serde_json::from_str(&selected_pass_ids).map_err(storage)?,
                    api_updated_at,
                    api_coverage_from,
                    api_coverage_to,
                    api_summary: api_summary
                        .map(|value| serde_json::from_str(&value))
                        .transpose()
                        .map_err(storage)?,
                    api_status: Some(api_status),
                    updated_at: Some(updated_at),
                    last_import_at,
                });
            }
        }
        let mut sales = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT group_id,sale_fingerprint,universe_id,asset_id,sale_at,sale_location,hold_status,price,revenue,import_batch_id,imported_at,external_transaction_id,source,dedupe_confidence FROM roblox_group_revenue_sales ORDER BY sale_at DESC,sale_fingerprint").map_err(storage)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(RobloxGroupSale {
                        group_id: row.get(0)?,
                        sale_fingerprint: row.get(1)?,
                        universe_id: row.get(2)?,
                        asset_id: row.get(3)?,
                        sale_at: row.get(4)?,
                        sale_location: row.get(5)?,
                        hold_status: row.get(6)?,
                        price: row.get::<_, i64>(7)? as u64,
                        revenue: row.get::<_, i64>(8)? as u64,
                        import_batch_id: row.get(9)?,
                        imported_at: row.get(10)?,
                        external_transaction_id: row.get(11)?,
                        source: row.get(12)?,
                        dedupe_confidence: row.get(13)?,
                    })
                })
                .map_err(storage)?;
            for row in rows {
                sales.push(row.map_err(storage)?);
            }
        }
        let mut passes = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT group_id,asset_id,universe_id,name,source FROM roblox_group_revenue_passes ORDER BY group_id,universe_id,name COLLATE NOCASE").map_err(storage)?;
            for row in stmt
                .query_map([], |row| {
                    Ok(RobloxRevenuePass {
                        group_id: row.get(0)?,
                        asset_id: row.get(1)?,
                        universe_id: row.get(2)?,
                        name: row.get(3)?,
                        source: row.get(4)?,
                    })
                })
                .map_err(storage)?
            {
                passes.push(row.map_err(storage)?);
            }
        }
        Ok(RobloxGroupRevenueData {
            groups,
            sales,
            passes,
        })
    }

    pub fn save_roblox_revenue_group(&self, group: &RobloxRevenueGroup) -> Result<(), Error> {
        validate_group(group)?;
        let experience_ids = serde_json::to_string(&group.experience_ids).map_err(storage)?;
        let pass_mappings = serde_json::to_string(&group.pass_mappings).map_err(storage)?;
        let selected_pass_ids = serde_json::to_string(&group.selected_pass_ids).map_err(storage)?;
        self.connection()?.execute("INSERT INTO roblox_group_revenue_groups(group_id,name,experience_ids_json,pass_mappings_json,selected_pass_ids_json) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(group_id) DO UPDATE SET name=excluded.name,experience_ids_json=excluded.experience_ids_json,pass_mappings_json=excluded.pass_mappings_json,selected_pass_ids_json=excluded.selected_pass_ids_json,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![group.group_id, group.name.trim(), experience_ids, pass_mappings, selected_pass_ids]).map_err(storage)?;
        Ok(())
    }

    pub fn import_roblox_group_sales(
        &self,
        group: &RobloxRevenueGroup,
        batch_id: &str,
        sales: &[RobloxGroupSale],
    ) -> Result<RobloxGroupRevenueImportResult, Error> {
        validate_group(group)?;
        if group.experience_ids.is_empty() {
            return Err(Error::InvalidContent(
                "Refresh Roblox group data first or enter experience IDs before importing a CSV."
                    .into(),
            ));
        }
        if batch_id.is_empty() || batch_id.len() > 100 || sales.is_empty() || sales.len() > 100_000
        {
            return Err(Error::InvalidContent(
                "The selected import is empty or too large.".into(),
            ));
        }
        for sale in sales {
            validate_sale(sale, group)?;
        }
        let mut conn = self.connection()?;
        let tx = conn.transaction().map_err(storage)?;
        let experience_ids = serde_json::to_string(&group.experience_ids).map_err(storage)?;
        let pass_mappings = serde_json::to_string(&group.pass_mappings).map_err(storage)?;
        let selected_pass_ids = serde_json::to_string(&group.selected_pass_ids).map_err(storage)?;
        tx.execute("INSERT INTO roblox_group_revenue_groups(group_id,name,experience_ids_json,pass_mappings_json,selected_pass_ids_json,last_import_at) VALUES(?1,?2,?3,?4,?5,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(group_id) DO UPDATE SET name=excluded.name,experience_ids_json=excluded.experience_ids_json,pass_mappings_json=excluded.pass_mappings_json,selected_pass_ids_json=excluded.selected_pass_ids_json,last_import_at=excluded.last_import_at,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![group.group_id, group.name.trim(), experience_ids, pass_mappings, selected_pass_ids]).map_err(storage)?;
        let mut inserted = 0;
        for sale in sales {
            inserted += tx.execute("INSERT OR IGNORE INTO roblox_group_revenue_sales(group_id,sale_fingerprint,universe_id,asset_id,sale_at,sale_location,hold_status,price,revenue,import_batch_id,external_transaction_id,source,dedupe_confidence) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)", params![group.group_id, sale.sale_fingerprint, sale.universe_id, sale.asset_id, sale.sale_at, sale.sale_location, sale.hold_status, sale.price as i64, sale.revenue as i64, batch_id, sale.external_transaction_id, sale.source, sale.dedupe_confidence]).map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(RobloxGroupRevenueImportResult {
            inserted,
            duplicates: sales.len() - inserted,
        })
    }

    pub fn apply_roblox_group_api_refresh(
        &self,
        group_id: &str,
        passes: &[RobloxRevenuePass],
        sales: &[RobloxGroupSale],
        coverage_from: Option<&str>,
        coverage_to: Option<&str>,
        summary: Option<&serde_json::Value>,
    ) -> Result<RobloxGroupRevenueImportResult, Error> {
        if passes.len() > 10_000 || sales.len() > 100_000 {
            return Err(Error::InvalidContent(
                "The Roblox response is too large.".into(),
            ));
        }
        let data = self.roblox_group_revenue()?;
        let mut group = data
            .groups
            .iter()
            .find(|entry| entry.group_id == group_id)
            .cloned()
            .ok_or_else(|| {
                Error::InvalidContent("Configure this group before refreshing Roblox data.".into())
            })?;
        let mut pass_ids = HashSet::new();
        for pass in passes {
            if pass.group_id != group_id
                || !valid_numeric_id(&pass.asset_id)
                || !valid_numeric_id(&pass.universe_id)
                || pass.name.trim().is_empty()
                || pass.name.chars().count() > 200
                || !pass_ids.insert(pass.asset_id.as_str())
            {
                return Err(Error::InvalidContent(
                    "The Roblox Game Pass catalog response is invalid.".into(),
                ));
            }
            if !group.experience_ids.contains(&pass.universe_id) {
                group.experience_ids.push(pass.universe_id.clone());
            }
        }
        group.experience_ids.sort_by(|left, right| left.cmp(right));
        for sale in sales {
            validate_sale(sale, &group)?;
        }
        let batch_id = format!("api-{}", uuid::Uuid::now_v7());
        let mut conn = self.connection()?;
        let tx = conn.transaction().map_err(storage)?;
        let experience_ids = serde_json::to_string(&group.experience_ids).map_err(storage)?;
        tx.execute(
            "DELETE FROM roblox_group_revenue_passes WHERE group_id=?1",
            [group_id],
        )
        .map_err(storage)?;
        for pass in passes {
            tx.execute("INSERT INTO roblox_group_revenue_passes(group_id,asset_id,universe_id,name,source) VALUES(?1,?2,?3,?4,?5)", params![group_id, pass.asset_id, pass.universe_id, pass.name.trim(), pass.source]).map_err(storage)?;
        }
        let mut inserted = 0;
        let mut duplicates = 0;
        let mut matched_field_rows: HashMap<String, usize> = HashMap::new();
        for sale in sales {
            let existing_id: Option<String> = if let Some(id) =
                sale.external_transaction_id.as_deref()
            {
                tx.query_row("SELECT sale_fingerprint FROM roblox_group_revenue_sales WHERE group_id=?1 AND external_transaction_id=?2", params![group_id, id], |row| row.get(0)).optional().map_err(storage)?
            } else {
                None
            };
            if existing_id.is_some() {
                duplicates += 1;
                continue;
            }
            let fields = serde_json::to_string(&[
                &sale.universe_id,
                &sale.asset_id,
                &sale.sale_at,
                &sale.sale_location,
                &sale.hold_status,
                &sale.price.to_string(),
                &sale.revenue.to_string(),
            ])
            .map_err(storage)?;
            let occurrence = matched_field_rows.entry(fields).or_default();
            let matching_rows: i64 = tx.query_row("SELECT COUNT(*) FROM roblox_group_revenue_sales WHERE group_id=?1 AND universe_id=?2 AND asset_id=?3 AND sale_at=?4 AND sale_location=?5 AND hold_status=?6 AND price=?7 AND revenue=?8", params![group_id,sale.universe_id,sale.asset_id,sale.sale_at,sale.sale_location,sale.hold_status,sale.price as i64,sale.revenue as i64], |row| row.get(0)).map_err(storage)?;
            if (*occurrence as i64) < matching_rows {
                if let Some(id) = sale.external_transaction_id.as_deref() {
                    tx.execute("UPDATE roblox_group_revenue_sales SET external_transaction_id=?1 WHERE rowid=(SELECT rowid FROM roblox_group_revenue_sales WHERE group_id=?2 AND universe_id=?3 AND asset_id=?4 AND sale_at=?5 AND sale_location=?6 AND hold_status=?7 AND price=?8 AND revenue=?9 AND external_transaction_id IS NULL LIMIT 1)", params![id,group_id,sale.universe_id,sale.asset_id,sale.sale_at,sale.sale_location,sale.hold_status,sale.price as i64,sale.revenue as i64]).map_err(storage)?;
                }
                *occurrence += 1;
                duplicates += 1;
                continue;
            }
            let confidence = if matching_rows > 0 || *occurrence > matching_rows as usize {
                "ambiguous"
            } else {
                sale.dedupe_confidence.as_str()
            };
            *occurrence += 1;
            let changed = tx.execute("INSERT OR IGNORE INTO roblox_group_revenue_sales(group_id,sale_fingerprint,universe_id,asset_id,sale_at,sale_location,hold_status,price,revenue,import_batch_id,external_transaction_id,source,dedupe_confidence) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'api',?12)", params![group_id,sale.sale_fingerprint,sale.universe_id,sale.asset_id,sale.sale_at,sale.sale_location,sale.hold_status,sale.price as i64,sale.revenue as i64,batch_id,sale.external_transaction_id,confidence]).map_err(storage)?;
            inserted += changed;
            if changed == 0 {
                duplicates += 1;
            }
        }
        let summary_json = summary
            .map(serde_json::to_string)
            .transpose()
            .map_err(storage)?;
        let status = if summary
            .and_then(|value| value.get("unsupportedTransactions"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            > 0
        {
            "partial"
        } else {
            "success"
        };
        tx.execute("UPDATE roblox_group_revenue_groups SET experience_ids_json=?1,api_updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),api_coverage_from=?2,api_coverage_to=?3,api_summary_json=?4,api_status=?5,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE group_id=?6", params![experience_ids,coverage_from,coverage_to,summary_json,status,group_id]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(RobloxGroupRevenueImportResult {
            inserted,
            duplicates,
        })
    }

    pub fn remove_roblox_group_revenue(&self, group_id: &str) -> Result<(), Error> {
        if !valid_numeric_id(group_id) {
            return Err(Error::InvalidContent("Enter a valid group ID.".into()));
        }
        self.connection()?
            .execute(
                "DELETE FROM roblox_group_revenue_groups WHERE group_id=?1",
                [group_id],
            )
            .map_err(storage)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn group() -> RobloxRevenueGroup {
        RobloxRevenueGroup {
            group_id: "16538500".into(),
            name: "APX".into(),
            experience_ids: vec!["84560840200510".into()],
            pass_mappings: BTreeMap::new(),
            selected_pass_ids: vec!["2053361604".into()],
            api_updated_at: None,
            api_coverage_from: None,
            api_coverage_to: None,
            api_summary: None,
            api_status: Some("never".into()),
            updated_at: None,
            last_import_at: None,
        }
    }

    fn sale(source: &str, id: &str, fingerprint: &str) -> RobloxGroupSale {
        RobloxGroupSale {
            group_id: String::new(),
            sale_fingerprint: fingerprint.into(),
            universe_id: "84560840200510".into(),
            asset_id: "2053361604".into(),
            sale_at: "2026-11-07T01:02:00.000Z".into(),
            sale_location: "Website".into(),
            hold_status: "Released".into(),
            price: 1000,
            revenue: 700,
            import_batch_id: "csv-batch".into(),
            external_transaction_id: Some(id.into()),
            source: source.into(),
            dedupe_confidence: "transaction_id".into(),
            imported_at: None,
        }
    }

    #[test]
    fn selected_passes_and_api_refresh_are_saved_and_cross_source_ids_deduplicate() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let group = group();
        workspace.save_roblox_revenue_group(&group).unwrap();
        let csv_sale = sale("csv", "transaction-1", &"a".repeat(64));
        workspace
            .import_roblox_group_sales(&group, "csv-batch", &[csv_sale])
            .unwrap();
        let pass = RobloxRevenuePass {
            group_id: group.group_id.clone(),
            asset_id: "2053361604".into(),
            universe_id: "84560840200510".into(),
            name: "EMS Fleet Pack".into(),
            source: "catalog".into(),
        };
        let api_sale = sale("api", "transaction-1", &"b".repeat(64));
        let result = workspace
            .apply_roblox_group_api_refresh(
                &group.group_id,
                &[pass],
                &[api_sale],
                Some("2026-11-07"),
                Some("2026-11-07"),
                Some(&serde_json::json!({"returnedTransactions":1})),
            )
            .unwrap();
        assert_eq!(result.inserted, 0);
        assert_eq!(result.duplicates, 1);
        let data = workspace.roblox_group_revenue().unwrap();
        assert_eq!(data.sales.len(), 1);
        assert_eq!(data.groups[0].selected_pass_ids, vec!["2053361604"]);
        assert_eq!(data.groups[0].api_status.as_deref(), Some("success"));
        assert_eq!(data.passes.len(), 1);
    }

    #[test]
    fn invalid_api_refresh_does_not_replace_the_last_successful_catalog_or_coverage() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let group = group();
        workspace.save_roblox_revenue_group(&group).unwrap();
        let good = RobloxRevenuePass {
            group_id: group.group_id.clone(),
            asset_id: "2053361604".into(),
            universe_id: "84560840200510".into(),
            name: "EMS Fleet Pack".into(),
            source: "catalog".into(),
        };
        workspace
            .apply_roblox_group_api_refresh(
                &group.group_id,
                &[good],
                &[],
                Some("2026-11-01"),
                Some("2026-11-07"),
                None,
            )
            .unwrap();
        let bad = RobloxRevenuePass {
            asset_id: "not-an-id".into(),
            ..RobloxRevenuePass {
                group_id: group.group_id.clone(),
                asset_id: "2053361604".into(),
                universe_id: "84560840200510".into(),
                name: "Invalid".into(),
                source: "catalog".into(),
            }
        };
        assert!(workspace
            .apply_roblox_group_api_refresh(&group.group_id, &[bad], &[], None, None, None)
            .is_err());
        let data = workspace.roblox_group_revenue().unwrap();
        assert_eq!(data.passes[0].name, "EMS Fleet Pack");
        assert_eq!(
            data.groups[0].api_coverage_from.as_deref(),
            Some("2026-11-01")
        );
    }

    #[test]
    fn field_only_overlap_is_multiset_matched_and_extra_identical_rows_are_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let group = group();
        workspace.save_roblox_revenue_group(&group).unwrap();
        let mut csv_sale = sale("csv", "unused", &"a".repeat(64));
        csv_sale.external_transaction_id = None;
        csv_sale.dedupe_confidence = "exact_fields".into();
        workspace
            .import_roblox_group_sales(&group, "csv-batch", &[csv_sale.clone()])
            .unwrap();
        let mut api_sale = sale("api", "unused", &"b".repeat(64));
        api_sale.external_transaction_id = None;
        api_sale.dedupe_confidence = "exact_fields".into();
        let mut second_api_sale = api_sale.clone();
        second_api_sale.sale_fingerprint = "c".repeat(64);
        let result = workspace
            .apply_roblox_group_api_refresh(
                &group.group_id,
                &[],
                &[api_sale, second_api_sale],
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(result.duplicates, 1);
        assert_eq!(result.inserted, 1);
        let data = workspace.roblox_group_revenue().unwrap();
        assert_eq!(data.sales.len(), 2);
        assert!(data
            .sales
            .iter()
            .any(|sale| sale.dedupe_confidence == "ambiguous"));
    }
}
