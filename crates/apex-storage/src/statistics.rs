use super::{storage, Error, OptionalExtension, Workspace};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsPost {
    pub bundle_id: u64,
    pub title: String,
    pub views: Option<u64>,
    pub likes: Option<u64>,
    pub copies: Option<u64>,
    pub favourites: Option<u64>,
    pub rating_count: Option<u64>,
    pub rating_average: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsSnapshot {
    pub collected_at: String,
    pub posts: Vec<StatisticsPost>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesPoint {
    pub date: String,
    pub sales: u64,
    pub net_robux: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesData {
    pub sales: u64,
    pub all_time_sales: u64,
    pub net_robux: u64,
    pub gross_robux: u64,
    pub series: Vec<SalesPoint>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesSnapshot {
    pub collected_at: String,
    pub data: SalesData,
}

pub fn validate_statistics_profile(id: &str) -> Result<(), Error> {
    if id.is_empty()
        || id.len() > 20
        || !id.bytes().all(|b| b.is_ascii_digit())
        || id.bytes().all(|b| b == b'0')
    {
        return Err(Error::InvalidContent(
            "Enter a valid Roblox user ID.".into(),
        ));
    }
    Ok(())
}

impl Workspace {
    pub fn statistics_profile(&self) -> Result<Option<String>, Error> {
        self.connection()?
            .query_row(
                "SELECT value FROM workspace_settings WHERE key='statistics_profile'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)
    }

    pub fn set_statistics_profile(&self, id: &str) -> Result<(), Error> {
        validate_statistics_profile(id)?;
        self.connection()?.execute("INSERT INTO workspace_settings(key,value) VALUES('statistics_profile',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [id]).map_err(storage)?;
        Ok(())
    }

    pub fn save_statistics_snapshot(
        &self,
        profile_id: &str,
        posts: &[StatisticsPost],
    ) -> Result<(), Error> {
        validate_statistics_profile(profile_id)?;
        let json = serde_json::to_string(posts).map_err(storage)?;
        let inserted = self.connection()?.execute("INSERT INTO statistics_snapshots(id, profile_id, posts_json) SELECT ?1,?2,?3 WHERE EXISTS(SELECT 1 FROM workspace_settings WHERE key='statistics_profile' AND value=?2)", params![uuid::Uuid::now_v7().to_string(), profile_id, json]).map_err(storage)?;
        if inserted != 1 {
            return Err(Error::Conflict);
        }
        Ok(())
    }

    pub fn statistics_snapshots(&self, profile_id: &str) -> Result<Vec<StatisticsSnapshot>, Error> {
        validate_statistics_profile(profile_id)?;
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT collected_at, posts_json FROM statistics_snapshots WHERE profile_id=?1 ORDER BY collected_at DESC, id DESC LIMIT 500").map_err(storage)?;
        let rows = stmt
            .query_map([profile_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(storage)?;
        let mut snapshots = Vec::new();
        for row in rows {
            let (collected_at, json) = row.map_err(storage)?;
            snapshots.push(StatisticsSnapshot {
                collected_at,
                posts: serde_json::from_str(&json).map_err(storage)?,
            });
        }
        snapshots.reverse();
        Ok(snapshots)
    }

    pub fn save_statistics_sales(&self, profile_id: &str, data: &SalesData) -> Result<(), Error> {
        validate_statistics_profile(profile_id)?;
        if data.series.len() > 31
            || data.series.iter().any(|p| {
                p.date.len() != 10
                    || !p.date.bytes().enumerate().all(|(i, b)| {
                        if i == 4 || i == 7 {
                            b == b'-'
                        } else {
                            b.is_ascii_digit()
                        }
                    })
            })
        {
            return Err(Error::InvalidContent("Invalid sales data.".into()));
        }
        let json = serde_json::to_string(data).map_err(storage)?;
        let inserted = self.connection()?.execute("INSERT INTO statistics_sales(id, profile_id, data_json) SELECT ?1,?2,?3 WHERE EXISTS(SELECT 1 FROM workspace_settings WHERE key='statistics_profile' AND value=?2)", params![uuid::Uuid::now_v7().to_string(), profile_id, json]).map_err(storage)?;
        if inserted != 1 {
            return Err(Error::Conflict);
        }
        Ok(())
    }

    pub fn statistics_sales(&self, profile_id: &str) -> Result<Option<SalesSnapshot>, Error> {
        validate_statistics_profile(profile_id)?;
        let row: Option<(String, String)> = self.connection()?.query_row("SELECT collected_at, data_json FROM statistics_sales WHERE profile_id=?1 ORDER BY collected_at DESC, id DESC LIMIT 1", [profile_id], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage)?;
        row.map(|(collected_at, json)| {
            Ok(SalesSnapshot {
                collected_at,
                data: serde_json::from_str(&json).map_err(storage)?,
            })
        })
        .transpose()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_are_scoped_and_survive_backup_restore() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("one")).unwrap();
        ws.set_statistics_profile("123").unwrap();
        let p = StatisticsPost {
            bundle_id: 42,
            title: "Example".into(),
            views: Some(10),
            likes: None,
            copies: Some(2),
            favourites: Some(0),
            rating_count: Some(0),
            rating_average: None,
        };
        ws.save_statistics_snapshot("123", &[p]).unwrap();
        let sales = SalesData {
            sales: 2,
            all_time_sales: 4,
            net_robux: 70,
            gross_robux: 100,
            series: vec![],
        };
        ws.save_statistics_sales("123", &sales).unwrap();
        let backup = dir.path().join("snapshot.apexbackup");
        ws.backup(&backup).unwrap();
        let restored = Workspace::open(dir.path().join("two")).unwrap();
        restored.restore_backup(&backup).unwrap();
        assert_eq!(
            restored.statistics_profile().unwrap().as_deref(),
            Some("123")
        );
        assert_eq!(
            restored.statistics_snapshots("123").unwrap()[0].posts[0].likes,
            None
        );
        assert_eq!(
            restored
                .statistics_sales("123")
                .unwrap()
                .unwrap()
                .data
                .net_robux,
            70
        );
        restored.set_statistics_profile("456").unwrap();
        assert!(matches!(
            restored.save_statistics_snapshot("123", &[]),
            Err(Error::Conflict)
        ));
        assert!(restored.statistics_snapshots("456").unwrap().is_empty());
        assert!(restored.statistics_sales("456").unwrap().is_none());
        assert!(matches!(
            restored.save_statistics_sales("123", &sales),
            Err(Error::Conflict)
        ));
    }
    #[test]
    fn profile_id_must_be_numeric_and_nonzero() {
        for value in ["", "000", "-1", "1/other", "123456789012345678901"] {
            assert!(validate_statistics_profile(value).is_err());
        }
        assert!(validate_statistics_profile("2919579206").is_ok());
    }
    #[test]
    fn early_schema_ten_migrates_missing_sales_table() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path()).unwrap();
        ws.set_statistics_profile("123").unwrap();
        ws.connection()
            .unwrap()
            .execute_batch("DROP TABLE post_markdown_media;DROP TABLE shared_decal_codes;DROP TABLE roblox_group_revenue_passes;DROP TABLE roblox_group_revenue_sales;DROP TABLE roblox_group_revenue_groups;DROP TABLE media_library;DROP TABLE overlay_templates;DROP TABLE markdown_presets;DROP TABLE overlay_render_versions;DROP TABLE statistics_sales;DROP TABLE project_sync;ALTER TABLE upload_jobs DROP COLUMN passes_required;ALTER TABLE managed_files DROP COLUMN local_available; PRAGMA user_version=10;")
            .unwrap();
        drop(ws);
        let reopened = Workspace::open(dir.path()).unwrap();
        assert_eq!(
            reopened.statistics_profile().unwrap().as_deref(),
            Some("123")
        );
        assert!(reopened.statistics_sales("123").unwrap().is_none());
    }
}
