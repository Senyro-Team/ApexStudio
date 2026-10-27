use super::{storage, Workspace};
use apex_core::Error;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
pub fn queue_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UploadBatch {
    pub id: String,
    pub account_id: String,
    pub state: String,
    pub created_at: String,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UploadJob {
    pub id: String,
    pub account_id: String,
    pub hash: String,
    pub name: String,
    pub state: String,
    pub pass: u8,
    pub next_at: i64,
    pub message: String,
    pub operation: Option<String>,
    pub first_id: Option<String>,
    pub second_id: Option<String>,
    pub moderation: String,
    pub generation: i64,
    pub poll_attempts: u32,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UploadItem {
    pub batch_id: String,
    pub job_id: String,
    pub folder_id: String,
    pub side: String,
}
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QueueSnapshot {
    pub batches: Vec<UploadBatch>,
    pub jobs: Vec<UploadJob>,
    pub items: Vec<UploadItem>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RobloxLiverySummary {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub name: String,
    pub vehicle_id: Option<String>,
    pub textures: u32,
    pub uploaded: u32,
}
fn job_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<UploadJob> {
    Ok(UploadJob {
        id: r.get(0)?,
        account_id: r.get(1)?,
        hash: r.get(2)?,
        name: r.get(3)?,
        state: r.get(4)?,
        pass: r.get(5)?,
        next_at: r.get(6)?,
        message: r.get(7)?,
        operation: r.get(8)?,
        first_id: r.get(9)?,
        second_id: r.get(10)?,
        moderation: r.get(11)?,
        generation: r.get(12)?,
        poll_attempts: r.get(13)?,
    })
}
const JOB_SELECT:&str="SELECT j.id,j.account_id,j.hash,j.name,j.state,j.pass,j.next_at,j.message,p.operation,p1.asset_id,p2.asset_id,COALESCE(p2.moderation,p1.moderation,'unknown'),j.generation,COALESCE(p.polls,0) FROM upload_jobs j LEFT JOIN upload_passes p ON p.job_id=j.id AND p.pass=j.pass LEFT JOIN upload_passes p1 ON p1.job_id=j.id AND p1.pass=1 LEFT JOIN upload_passes p2 ON p2.job_id=j.id AND p2.pass=2";
impl Workspace {
    pub fn roblox_liveries(&self, account: &str) -> Result<Vec<RobloxLiverySummary>, Error> {
        let db = self.connection()?;
        let mut q=db.prepare("SELECT f.id,f.project_id,json_extract(pr.metadata,'$.name'),f.name,f.vehicle_id,count(p.asset_id),sum(CASE WHEN u.asset_id IS NOT NULL AND u.moderation!='rejected' THEN 1 ELSE 0 END) FROM asset_folders f JOIN projects pr ON pr.id=f.project_id LEFT JOIN asset_placements p ON p.folder_id=f.id AND p.slot IS NOT NULL LEFT JOIN assets a ON a.id=p.asset_id LEFT JOIN roblox_uploads u ON u.account_id=?1 AND u.hash=a.hash WHERE f.kind='livery' AND pr.archived=0 GROUP BY f.id ORDER BY pr.updated_at DESC,f.name COLLATE NOCASE").map_err(storage)?;
        let result = q
            .query_map([account], |r| {
                Ok(RobloxLiverySummary {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    project_name: r.get(2)?,
                    name: r.get(3)?,
                    vehicle_id: r.get(4)?,
                    textures: r.get(5)?,
                    uploaded: r.get::<_, Option<u32>>(6)?.unwrap_or(0),
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage);
        result
    }
    pub fn create_upload_batch(
        &self,
        id: &str,
        account: &str,
        folders: &[String],
        force: bool,
    ) -> Result<String, Error> {
        if folders.is_empty() || folders.len() > 500 || id.len() > 100 || id.is_empty() {
            return Err(Error::InvalidContent("Select 1–500 liveries.".into()));
        }
        // Fetch lightweight immutable snapshots before taking the short writer transaction.
        let mut all = Vec::new();
        for folder in folders {
            let f = self.folder(folder)?;
            self.editable_project(&f.project_id)?;
            let plan = self.livery_uploads(folder, account)?;
            for t in plan.textures {
                all.push((folder.clone(), f.name.clone(), t));
            }
        }
        if all.is_empty() {
            return Err(Error::InvalidContent(
                "Assign texture sides before uploading.".into(),
            ));
        }
        let mut db = self.connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT account_id FROM upload_batches WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        if let Some(owner) = existing {
            if owner != account {
                return Err(Error::Conflict);
            }
            return Ok(id.into());
        }
        tx.execute(
            "INSERT INTO upload_batches(id,account_id,state)VALUES(?1,?2,'running')",
            params![id, account],
        )
        .map_err(storage)?;
        let mut linked = HashMap::<String, String>::new();
        for (folder, name, t) in all {
            let active:Option<String>=tx.query_row("SELECT id FROM upload_jobs WHERE account_id=?1 AND hash=?2 AND state NOT IN ('complete','cancelled')",params![account,t.hash],|r|r.get(0)).optional().map_err(storage)?;
            let job = if let Some(job) = linked.get(&t.hash) {
                job.clone()
            } else if let Some(job) = active {
                job
            } else {
                let reusable = t.upload.as_ref().is_some_and(|u| {
                    u.state == "uploaded" && u.asset_id.is_some() && u.moderation != "rejected"
                });
                if reusable && !force {
                    continue;
                }
                let job = uuid::Uuid::now_v7().to_string();
                let generation:i64=tx.query_row("SELECT COALESCE(max(generation),0)+1 FROM upload_jobs WHERE account_id=?1 AND hash=?2",params![account,t.hash],|r|r.get(0)).map_err(storage)?;
                let legacy = t.upload.as_ref().filter(|_| !force);
                let uncertain = legacy.is_some_and(|u| {
                    ["uploading", "uncertain"].contains(&u.state.as_str()) && u.operation.is_none()
                });
                let operation = legacy.and_then(|u| u.operation.as_deref());
                let state = if uncertain {
                    "needs_review"
                } else if operation.is_some() {
                    "processing"
                } else {
                    "queued"
                };
                let display = format!("{name} — {}", t.side)
                    .chars()
                    .take(50)
                    .collect::<String>();
                tx.execute("INSERT INTO upload_jobs(id,account_id,hash,name,state,generation,message)VALUES(?1,?2,?3,?4,?5,?6,?7)",params![job,account,t.hash,display,state,generation,if uncertain{"Earlier upload has an unknown result. Check Creator Dashboard before retrying."}else{""}]).map_err(storage)?;
                tx.execute(
                    "INSERT INTO upload_passes(job_id,pass,state,operation)VALUES(?1,1,?2,?3)",
                    params![job, state, operation],
                )
                .map_err(storage)?;
                job
            };
            linked.insert(t.hash, job.clone());
            tx.execute("INSERT OR IGNORE INTO upload_batch_items(batch_id,job_id,folder_id,side)VALUES(?1,?2,?3,?4)",params![id,job,folder,t.side]).map_err(storage)?;
        }
        let count: i64 = tx
            .query_row(
                "SELECT count(*) FROM upload_batch_items WHERE batch_id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if count == 0 {
            tx.execute(
                "UPDATE upload_batches SET state='complete' WHERE id=?1",
                [id],
            )
            .map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(id.into())
    }
    pub fn upload_snapshot(&self) -> Result<QueueSnapshot, Error> {
        let connection = self.connection()?;
        let db = connection.unchecked_transaction().map_err(storage)?;
        let mut b=db.prepare("SELECT id,account_id,state,created_at FROM upload_batches ORDER BY created_at DESC,id DESC LIMIT 100").map_err(storage)?;
        let batches = b
            .query_map([], |r| {
                Ok(UploadBatch {
                    id: r.get(0)?,
                    account_id: r.get(1)?,
                    state: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        let mut j=db.prepare(&format!("{JOB_SELECT} WHERE j.id IN(SELECT job_id FROM upload_batch_items WHERE batch_id IN(SELECT id FROM upload_batches ORDER BY created_at DESC,id DESC LIMIT 100)) ORDER BY j.id DESC LIMIT 5000")).map_err(storage)?;
        let jobs = j
            .query_map([], job_row)
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        let mut i=db.prepare("SELECT batch_id,job_id,folder_id,side FROM upload_batch_items WHERE batch_id IN(SELECT id FROM upload_batches ORDER BY created_at DESC,id DESC LIMIT 100)").map_err(storage)?;
        let items = i
            .query_map([], |r| {
                Ok(UploadItem {
                    batch_id: r.get(0)?,
                    job_id: r.get(1)?,
                    folder_id: r.get(2)?,
                    side: r.get(3)?,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        Ok(QueueSnapshot {
            batches,
            jobs,
            items,
        })
    }
    pub fn lease_upload_tasks(&self, poll: bool, limit: usize) -> Result<Vec<UploadJob>, Error> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut db = self.connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let sql=format!("{JOB_SELECT} WHERE j.state=?1 AND j.next_at<=?2 AND EXISTS(SELECT 1 FROM upload_batch_items i JOIN upload_batches b ON b.id=i.batch_id WHERE i.job_id=j.id AND b.state='running') ORDER BY j.pass DESC,j.id LIMIT ?3");
        let tasks = tx
            .prepare(&sql)
            .map_err(storage)?
            .query_map(
                params![
                    if poll { "processing" } else { "queued" },
                    queue_now(),
                    limit as u32
                ],
                job_row,
            )
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        for job in &tasks {
            tx.execute(
                "UPDATE upload_jobs SET state=?2 WHERE id=?1",
                params![job.id, if poll { "polling" } else { "submitting" }],
            )
            .map_err(storage)?;
            if poll {
                tx.execute(
                    "UPDATE upload_passes SET polls=polls+1 WHERE job_id=?1 AND pass=?2",
                    params![job.id, job.pass],
                )
                .map_err(storage)?;
            }
            if !poll {
                tx.execute("UPDATE upload_passes SET state='submitting',attempts=attempts+1 WHERE job_id=?1 AND pass=?2",params![job.id,job.pass]).map_err(storage)?;
                tx.execute("INSERT INTO upload_attempts(job_id,pass,attempt,state) SELECT job_id,pass,attempts,'submitting' FROM upload_passes WHERE job_id=?1 AND pass=?2",params![job.id,job.pass]).map_err(storage)?;
            }
        }
        tx.commit().map_err(storage)?;
        Ok(tasks)
    }
    pub fn upload_operation(&self, job: &UploadJob, operation: &str) -> Result<(), Error> {
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(storage)?;
        tx.execute(
            "UPDATE upload_passes SET operation=?3,state='processing' WHERE job_id=?1 AND pass=?2",
            params![job.id, job.pass, operation],
        )
        .map_err(storage)?;
        tx.execute("UPDATE upload_attempts SET operation=?3,state='processing' WHERE job_id=?1 AND pass=?2 AND attempt=(SELECT attempts FROM upload_passes WHERE job_id=?1 AND pass=?2)",params![job.id,job.pass,operation]).map_err(storage)?;
        tx.execute(
            "UPDATE upload_jobs SET state='processing',next_at=?2,message='' WHERE id=?1",
            params![job.id, queue_now() + 2000],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(())
    }
    pub fn upload_defer(
        &self,
        job: &UploadJob,
        state: &str,
        delay: i64,
        message: &str,
    ) -> Result<(), Error> {
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(storage)?;
        tx.execute(
            "UPDATE upload_jobs SET state=?2,next_at=?3,message=?4 WHERE id=?1",
            params![job.id, state, queue_now() + delay, message],
        )
        .map_err(storage)?;
        tx.execute("UPDATE upload_attempts SET state=?3 WHERE job_id=?1 AND pass=?2 AND attempt=(SELECT attempts FROM upload_passes WHERE job_id=?1 AND pass=?2)",params![job.id,job.pass,state]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(())
    }
    pub fn upload_pass_complete(
        &self,
        job: &UploadJob,
        id: &str,
        moderation: &str,
    ) -> Result<(), Error> {
        let mut db = self.connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        tx.execute("UPDATE upload_passes SET state='complete',asset_id=?3,moderation=?4 WHERE job_id=?1 AND pass=?2",params![job.id,job.pass,id,moderation]).map_err(storage)?;
        tx.execute("UPDATE upload_attempts SET asset_id=?3,state='complete' WHERE job_id=?1 AND pass=?2 AND attempt=(SELECT attempts FROM upload_passes WHERE job_id=?1 AND pass=?2)",params![job.id,job.pass,id]).map_err(storage)?;
        if moderation == "rejected" {
            tx.execute("UPDATE upload_jobs SET state='failed',message='Roblox rejected this decal in moderation.' WHERE id=?1",[&job.id]).map_err(storage)?;
        } else if job.pass == 1 {
            tx.execute(
                "INSERT OR IGNORE INTO upload_passes(job_id,pass,state)VALUES(?1,2,'queued')",
                [&job.id],
            )
            .map_err(storage)?;
            tx.execute(
                "UPDATE upload_jobs SET pass=2,state='queued',next_at=0,message='' WHERE id=?1",
                [&job.id],
            )
            .map_err(storage)?;
        } else {
            tx.execute(
                "UPDATE upload_jobs SET state='complete',message='' WHERE id=?1",
                [&job.id],
            )
            .map_err(storage)?;
            tx.execute("INSERT INTO roblox_uploads(account_id,hash,state,asset_id,moderation)VALUES(?1,?2,'uploaded',?3,?4) ON CONFLICT(account_id,hash) DO UPDATE SET state='uploaded',asset_id=excluded.asset_id,moderation=excluded.moderation,operation=NULL,message=''",params![job.account_id,job.hash,id,moderation]).map_err(storage)?;
        }
        tx.execute("UPDATE upload_batches SET state='complete' WHERE state IN ('running','paused') AND NOT EXISTS(SELECT 1 FROM upload_batch_items i JOIN upload_jobs j ON j.id=i.job_id WHERE i.batch_id=upload_batches.id AND j.state!='complete')",[]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(())
    }
    pub fn pause_upload_account(&self, account: &str) -> Result<(), Error> {
        self.connection()?
            .execute(
                "UPDATE upload_batches SET state='paused' WHERE account_id=?1 AND state='running'",
                [account],
            )
            .map_err(storage)?;
        Ok(())
    }
    pub fn control_upload_batch(&self, id: Option<&str>, action: &str) -> Result<(), Error> {
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(storage)?;
        match action {
            "pause" | "resume" => {
                tx.execute("UPDATE upload_batches SET state=?2 WHERE (?1 IS NULL OR id=?1) AND state IN ('running','paused')",params![id,if action=="pause"{"paused"}else{"running"}]).map_err(storage)?;
            }
            "cancel" => {
                tx.execute("UPDATE upload_batches SET state='cancelled' WHERE (?1 IS NULL OR id=?1) AND state IN ('running','paused')",[id]).map_err(storage)?;
                tx.execute("UPDATE upload_jobs SET state='cancelled' WHERE state IN ('queued','processing','failed','needs_review') AND NOT EXISTS(SELECT 1 FROM upload_batch_items i JOIN upload_batches b ON b.id=i.batch_id WHERE i.job_id=upload_jobs.id AND b.state IN ('running','paused'))",[]).map_err(storage)?;
            }
            "retry" => {
                tx.execute("UPDATE upload_jobs SET state=CASE WHEN EXISTS(SELECT 1 FROM upload_passes p WHERE p.job_id=upload_jobs.id AND p.pass=upload_jobs.pass AND p.operation IS NOT NULL AND p.state!='failed') THEN 'processing' ELSE 'queued' END,next_at=0,message='' WHERE state='failed' AND id IN(SELECT job_id FROM upload_batch_items WHERE ?1 IS NULL OR batch_id=?1)",[id]).map_err(storage)?;
                tx.execute("UPDATE upload_batches SET state='running' WHERE (?1 IS NULL OR id=?1) AND state='paused'",[id]).map_err(storage)?;
            }
            _ => return Err(Error::InvalidContent("Unknown upload action.".into())),
        }
        tx.commit().map_err(storage)?;
        Ok(())
    }
    pub fn fail_upload_operation(&self, job: &UploadJob) -> Result<(), Error> {
        self.connection()?.execute("UPDATE upload_passes SET operation=NULL,state='failed' WHERE job_id=?1 AND pass=?2",params![job.id,job.pass]).map_err(storage)?;
        self.upload_defer(
            job,
            "failed",
            0,
            "Roblox could not process this texture. Retry this pass after checking the file.",
        )
    }
    pub fn recover_queue(&self) -> Result<(), Error> {
        self.connection()?.execute_batch("BEGIN IMMEDIATE;UPDATE upload_batches SET state='paused' WHERE state='running';UPDATE upload_attempts SET state='needs_review' WHERE state='submitting';UPDATE upload_jobs SET state='needs_review',message='Upload result unknown after interruption. Check Creator Dashboard.' WHERE state='submitting';UPDATE upload_jobs SET state='processing' WHERE state='polling';COMMIT;").map_err(storage)
    }
    pub fn upload_running(&self) -> Result<bool, Error> {
        self.connection()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM upload_batches WHERE state='running')",
                [],
                |r| r.get(0),
            )
            .map_err(storage)
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Workspace, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                apex_core::ProjectInput {
                    name: "Queue".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let v = apex_core::vehicle_catalogue().unwrap().vehicles.remove(0);
        let path = dir.path().join("Left1.png");
        image::RgbImage::new(3, 2).save(&path).unwrap();
        let f = ws
            .import_livery(
                &p.id,
                None,
                None,
                "Patrol",
                &v.id,
                &[(path, "Left1.png".into(), Some("Left 1".into()))],
                None,
            )
            .unwrap();
        let hash = ws.livery_uploads(&f, "123").unwrap().textures[0]
            .hash
            .clone();
        (dir, ws, f, hash)
    }
    fn create(ws: &Workspace, f: &str, id: &str, force: bool) {
        ws.create_upload_batch(id, "123", &[f.into()], force)
            .unwrap();
    }
    fn first(ws: &Workspace) -> UploadJob {
        ws.lease_upload_tasks(false, 5).unwrap().remove(0)
    }
    #[test]
    fn duplicate_batches_and_accounts_are_isolated() {
        let (_d, ws, f, _) = fixture();
        assert_eq!(ws.roblox_liveries("123").unwrap().len(), 1);
        create(&ws, &f, "one", false);
        create(&ws, &f, "one", false);
        create(&ws, &f, "two", false);
        assert_eq!(ws.upload_snapshot().unwrap().jobs.len(), 1);
        assert_eq!(ws.upload_snapshot().unwrap().items.len(), 2);
        assert_eq!(ws.lease_upload_tasks(false, 10).unwrap().len(), 1);
        assert!(ws.lease_upload_tasks(false, 10).unwrap().is_empty());
        ws.create_upload_batch("other", "456", &[f], false).unwrap();
        assert_eq!(ws.upload_snapshot().unwrap().jobs.len(), 2);
    }
    #[test]
    fn second_pass_only_is_selected_and_retry_preserves_first() {
        let (_d, ws, f, h) = fixture();
        create(&ws, &f, "one", false);
        let j = first(&ws);
        assert_eq!(j.pass, 1);
        assert!(ws.lease_upload_tasks(false, 10).unwrap().is_empty());
        ws.upload_operation(&j, "operations/first").unwrap();
        assert!(ws.lease_upload_tasks(false, 10).unwrap().is_empty());
        ws.upload_pass_complete(&j, "111", "reviewing").unwrap();
        assert!(ws.livery_uploads(&f, "123").unwrap().textures[0]
            .upload
            .is_none());
        let j = first(&ws);
        assert_eq!(j.pass, 2);
        ws.upload_defer(&j, "failed", 0, "Failed second").unwrap();
        ws.control_upload_batch(Some("one"), "retry").unwrap();
        let j = first(&ws);
        assert_eq!(j.pass, 2);
        ws.upload_pass_complete(&j, "222", "reviewing").unwrap();
        let s = ws.upload_snapshot().unwrap();
        assert_eq!(s.jobs[0].first_id.as_deref(), Some("111"));
        assert_eq!(s.jobs[0].second_id.as_deref(), Some("222"));
        assert_eq!(
            ws.livery_uploads(&f, "123").unwrap().textures[0]
                .upload
                .as_ref()
                .unwrap()
                .asset_id
                .as_deref(),
            Some("222")
        );
        assert_eq!(s.batches[0].state, "complete");
        assert!(!ws.decal_bytes(&h).unwrap().is_empty());
    }
    #[test]
    fn legacy_reuse_and_failed_reupload_keep_previous_selected_id() {
        let (_d, ws, f, h) = fixture();
        ws.claim_decal(&f, "123", &h).unwrap();
        ws.save_decal_upload(&super::super::DecalUpload {
            account_id: "123".into(),
            hash: h,
            state: "uploaded".into(),
            asset_id: Some("99".into()),
            operation: None,
            moderation: "approved".into(),
            message: "".into(),
        })
        .unwrap();
        create(&ws, &f, "reuse", false);
        assert!(ws.upload_snapshot().unwrap().jobs.is_empty());
        create(&ws, &f, "force", true);
        let j = first(&ws);
        ws.upload_pass_complete(&j, "111", "approved").unwrap();
        let j = first(&ws);
        ws.upload_defer(&j, "failed", 0, "fail").unwrap();
        assert_eq!(
            ws.livery_uploads(&f, "123").unwrap().textures[0]
                .upload
                .as_ref()
                .unwrap()
                .asset_id
                .as_deref(),
            Some("99")
        );
    }
    #[test]
    fn restart_pauses_and_unknown_post_never_retries() {
        let (_d, ws, f, _) = fixture();
        create(&ws, &f, "one", false);
        let _j = first(&ws);
        ws.recover_queue().unwrap();
        let s = ws.upload_snapshot().unwrap();
        assert_eq!(s.batches[0].state, "paused");
        assert_eq!(s.jobs[0].state, "needs_review");
        ws.control_upload_batch(None, "resume").unwrap();
        ws.control_upload_batch(None, "retry").unwrap();
        assert!(ws.lease_upload_tasks(false, 10).unwrap().is_empty());
    }
    #[test]
    fn cancelling_one_batch_does_not_cancel_shared_work() {
        let (_d, ws, f, _) = fixture();
        create(&ws, &f, "one", false);
        create(&ws, &f, "two", false);
        ws.control_upload_batch(Some("one"), "cancel").unwrap();
        assert_eq!(first(&ws).pass, 1);
    }
    #[test]
    fn backup_restores_queue_paused_and_discards_validation_trust() {
        let (d, ws, f, _) = fixture();
        create(&ws, &f, "one", false);
        let j = first(&ws);
        ws.upload_operation(&j, "operations/first").unwrap();
        let backup = d.path().join("queue.apexbackup");
        ws.backup(&backup).unwrap();
        let restored = Workspace::open(d.path().join("restored")).unwrap();
        restored.restore_backup(&backup).unwrap();
        let s = restored.upload_snapshot().unwrap();
        assert_eq!(s.batches[0].state, "paused");
        assert_eq!(s.jobs[0].operation.as_deref(), Some("operations/first"));
        let n: u32 = restored
            .connection()
            .unwrap()
            .query_row("SELECT count(*) FROM image_validation", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        assert!(restored.lease_upload_tasks(true, 5).unwrap().is_empty());
    }
    #[test]
    fn rejected_first_pass_does_not_queue_second() {
        let (_d, ws, f, _) = fixture();
        create(&ws, &f, "one", false);
        let j = first(&ws);
        ws.upload_pass_complete(&j, "111", "rejected").unwrap();
        assert!(ws.lease_upload_tasks(false, 10).unwrap().is_empty());
        assert_eq!(ws.upload_snapshot().unwrap().jobs[0].state, "failed");
    }
}
