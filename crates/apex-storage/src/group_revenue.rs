use super::{storage, Error, Workspace};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RobloxRevenueGroup {
    pub group_id: String,
    pub name: String,
    pub experience_ids: Vec<String>,
    pub pass_mappings: BTreeMap<String, String>,
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
    #[serde(skip_deserializing)]
    pub imported_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RobloxGroupRevenueData {
    pub groups: Vec<RobloxRevenueGroup>,
    pub sales: Vec<RobloxGroupSale>,
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
        || group.experience_ids.is_empty()
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
    if group.pass_mappings.len() > 1000
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
            let mut stmt = conn.prepare("SELECT group_id,name,experience_ids_json,pass_mappings_json,updated_at,last_import_at FROM roblox_group_revenue_groups ORDER BY name COLLATE NOCASE,group_id").map_err(storage)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })
                .map_err(storage)?;
            for row in rows {
                let (group_id, name, experience_ids, pass_mappings, updated_at, last_import_at) =
                    row.map_err(storage)?;
                groups.push(RobloxRevenueGroup {
                    group_id,
                    name,
                    experience_ids: serde_json::from_str(&experience_ids).map_err(storage)?,
                    pass_mappings: serde_json::from_str(&pass_mappings).map_err(storage)?,
                    updated_at: Some(updated_at),
                    last_import_at,
                });
            }
        }
        let mut sales = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT group_id,sale_fingerprint,universe_id,asset_id,sale_at,sale_location,hold_status,price,revenue,import_batch_id,imported_at FROM roblox_group_revenue_sales ORDER BY sale_at DESC,sale_fingerprint").map_err(storage)?;
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
                    })
                })
                .map_err(storage)?;
            for row in rows {
                sales.push(row.map_err(storage)?);
            }
        }
        Ok(RobloxGroupRevenueData { groups, sales })
    }

    pub fn save_roblox_revenue_group(&self, group: &RobloxRevenueGroup) -> Result<(), Error> {
        validate_group(group)?;
        let experience_ids = serde_json::to_string(&group.experience_ids).map_err(storage)?;
        let pass_mappings = serde_json::to_string(&group.pass_mappings).map_err(storage)?;
        self.connection()?.execute("INSERT INTO roblox_group_revenue_groups(group_id,name,experience_ids_json,pass_mappings_json) VALUES(?1,?2,?3,?4) ON CONFLICT(group_id) DO UPDATE SET name=excluded.name,experience_ids_json=excluded.experience_ids_json,pass_mappings_json=excluded.pass_mappings_json,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![group.group_id, group.name.trim(), experience_ids, pass_mappings]).map_err(storage)?;
        Ok(())
    }

    pub fn import_roblox_group_sales(
        &self,
        group: &RobloxRevenueGroup,
        batch_id: &str,
        sales: &[RobloxGroupSale],
    ) -> Result<RobloxGroupRevenueImportResult, Error> {
        validate_group(group)?;
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
        tx.execute("INSERT INTO roblox_group_revenue_groups(group_id,name,experience_ids_json,pass_mappings_json,last_import_at) VALUES(?1,?2,?3,?4,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(group_id) DO UPDATE SET name=excluded.name,experience_ids_json=excluded.experience_ids_json,pass_mappings_json=excluded.pass_mappings_json,last_import_at=excluded.last_import_at,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![group.group_id, group.name.trim(), experience_ids, pass_mappings]).map_err(storage)?;
        let mut inserted = 0;
        for sale in sales {
            inserted += tx.execute("INSERT OR IGNORE INTO roblox_group_revenue_sales(group_id,sale_fingerprint,universe_id,asset_id,sale_at,sale_location,hold_status,price,revenue,import_batch_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)", params![group.group_id, sale.sale_fingerprint, sale.universe_id, sale.asset_id, sale.sale_at, sale.sale_location, sale.hold_status, sale.price as i64, sale.revenue as i64, batch_id]).map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(RobloxGroupRevenueImportResult {
            inserted,
            duplicates: sales.len() - inserted,
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
