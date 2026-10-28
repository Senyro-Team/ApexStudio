use super::{storage, Workspace};
use apex_core::Error;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, Weak},
};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashItem {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub root_id: String,
    pub kind: String,
    pub name: String,
    pub deleted_at: String,
    pub files: u32,
    pub folders: u32,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashPreview {
    pub name: String,
    pub files: u32,
    pub folders: u32,
}
type TrashSelection = (String, String, Option<String>, Vec<String>, Vec<String>);
impl Workspace {
    pub(super) fn content_gate(&self) -> Arc<parking_lot::ReentrantMutex<()>> {
        type Gates = HashMap<PathBuf, Weak<parking_lot::ReentrantMutex<()>>>;
        static GATES: OnceLock<Mutex<Gates>> = OnceLock::new();
        let mut gates = GATES
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        gates.retain(|_, g| g.strong_count() > 0);
        if let Some(g) = gates.get(&self.root).and_then(Weak::upgrade) {
            return g;
        }
        let g = Arc::new(parking_lot::ReentrantMutex::new(()));
        gates.insert(self.root.clone(), Arc::downgrade(&g));
        g
    }
    pub fn pin_content(&self, owner: &str, hashes: &[String]) -> Result<(), Error> {
        let gate = self.content_gate();
        let _guard = gate.lock();
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(storage)?;
        for h in hashes {
            if !apex_core::valid_hash(h) {
                return Err(Error::NotFound);
            }
            tx.execute("INSERT INTO content_leases(owner,hash,expires_at) VALUES(?1,?2,?3) ON CONFLICT(owner,hash) DO UPDATE SET expires_at=excluded.expires_at",params![owner,h,super::queue_now()+86_400_000]).map_err(storage)?;
        }
        tx.commit().map_err(storage)
    }
    pub fn release_content(&self, owner: &str) -> Result<(), Error> {
        self.connection()?
            .execute("DELETE FROM content_leases WHERE owner=?1", [owner])
            .map_err(storage)?;
        Ok(())
    }
    fn trash_selection(
        &self,
        id: &str,
        folder: bool,
    ) -> Result<TrashSelection, Error> {
        let db = self.connection()?;
        if folder {
            let f = self.folder(id)?;
            self.editable_project(&f.project_id)?;
            let mut q=db.prepare("WITH RECURSIVE d(id) AS(SELECT id FROM asset_folders WHERE id=?1 AND trash_id IS NULL UNION ALL SELECT f.id FROM asset_folders f JOIN d ON f.parent_id=d.id WHERE f.trash_id IS NULL) SELECT id FROM d").map_err(storage)?;
            let folders = q
                .query_map([id], |r| r.get::<_, String>(0))
                .map_err(storage)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage)?;
            let board = self.board(&f.project_id)?;
            let assets = board
                .placements
                .into_iter()
                .filter(|p| p.folder_id.as_ref().is_some_and(|f| folders.contains(f)))
                .map(|p| p.asset_id)
                .collect();
            Ok((f.project_id, f.name, f.parent_id, folders, assets))
        } else {
            let row=db.query_row("SELECT a.project_id,a.name,p.folder_id FROM assets a LEFT JOIN asset_placements p ON p.asset_id=a.id WHERE a.id=?1 AND a.trash_id IS NULL",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?))).optional().map_err(storage)?.ok_or(Error::NotFound)?;
            self.editable_project(&row.0)?;
            Ok((row.0, row.1, row.2, vec![], vec![id.into()]))
        }
    }
    pub fn trash_preview(&self, id: &str, folder: bool) -> Result<TrashPreview, Error> {
        let (_, name, _, folders, assets) = self.trash_selection(id, folder)?;
        Ok(TrashPreview {
            name,
            files: assets.len() as u32,
            folders: folders.len() as u32,
        })
    }
    pub fn trash_item(&self, id: &str, folder: bool) -> Result<String, Error> {
        let gate = self.content_gate();
        let _guard = gate.lock();
        let (project, name, parent, folders, assets) = self.trash_selection(id, folder)?;
        let key = uuid::Uuid::now_v7().to_string();
        let mut db = self.connection()?;
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        tx.execute("INSERT INTO trash_entries(id,project_id,root_id,kind,name,parent_id) VALUES(?1,?2,?3,?4,?5,?6)",params![key,project,id,if folder{"folder"}else{"file"},name,parent]).map_err(storage)?;
        for f in &folders {
            tx.execute(
                "UPDATE asset_folders SET trash_id=?1 WHERE id=?2",
                params![key, f],
            )
            .map_err(storage)?;
            tx.execute(
                "UPDATE upload_batch_items SET enabled=0 WHERE folder_id=?1",
                [f],
            )
            .map_err(storage)?;
        }
        for a in &assets {
            tx.execute(
                "UPDATE asset_placements SET active=0 WHERE asset_id=?1",
                [a],
            )
            .map_err(storage)?;
            tx.execute("UPDATE assets SET trash_id=?1 WHERE id=?2", params![key, a])
                .map_err(storage)?;
        }
        // Side-level removal also invalidates that exact source in every queued snapshot.
        tx.execute("UPDATE upload_batch_items SET enabled=0 WHERE EXISTS(SELECT 1 FROM assets a JOIN asset_placements p ON p.asset_id=a.id JOIN upload_jobs j ON j.hash=a.hash WHERE a.trash_id=?1 AND p.folder_id=upload_batch_items.folder_id AND p.slot=upload_batch_items.side AND j.id=upload_batch_items.job_id)",[&key]).map_err(storage)?;
        tx.execute("UPDATE upload_jobs SET state='cancelled',message='Source moved to trash.' WHERE state IN ('queued','processing','failed','needs_review') AND NOT EXISTS(SELECT 1 FROM upload_batch_items i JOIN upload_batches b ON b.id=i.batch_id WHERE i.job_id=upload_jobs.id AND i.enabled=1 AND b.state IN ('running','paused'))",[]).map_err(storage)?;
        tx.execute("UPDATE upload_batches SET state='complete' WHERE state IN ('running','paused') AND NOT EXISTS(SELECT 1 FROM upload_batch_items i JOIN upload_jobs j ON j.id=i.job_id WHERE i.batch_id=upload_batches.id AND i.enabled=1 AND j.state NOT IN ('complete','cancelled'))",[]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(key)
    }
    pub fn trash_items(&self) -> Result<Vec<TrashItem>, Error> {
        let db = self.connection()?;
        let mut q=db.prepare("SELECT t.id,t.project_id,json_extract(p.metadata,'$.name'),t.root_id,t.kind,t.name,t.deleted_at,(SELECT count(*) FROM assets WHERE trash_id=t.id),(SELECT count(*) FROM asset_folders WHERE trash_id=t.id) FROM trash_entries t JOIN projects p ON p.id=t.project_id ORDER BY t.deleted_at DESC,t.id DESC").map_err(storage)?;
        let out = q
            .query_map([], |r| {
                Ok(TrashItem {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    project_name: r.get(2)?,
                    root_id: r.get(3)?,
                    kind: r.get(4)?,
                    name: r.get(5)?,
                    deleted_at: r.get(6)?,
                    files: r.get(7)?,
                    folders: r.get(8)?,
                })
            })
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage);
        out
    }
    pub fn restore_trash(&self, id: &str) -> Result<String, Error> {
        let gate = self.content_gate();
        let _guard = gate.lock();
        let mut db = self.connection()?;
        let (project, root, kind, name, parent): (String, String, String, String, Option<String>) =
            db.query_row(
                "SELECT project_id,root_id,kind,name,parent_id FROM trash_entries WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        self.editable_project(&project)?;
        let parent = parent.filter(|p| self.folder(p).is_ok());
        let mut restored = name.clone();
        let mut index = 1;
        loop {
            let collision: bool = if kind == "folder" {
                db.query_row("SELECT EXISTS(SELECT 1 FROM asset_folders WHERE project_id=?1 AND parent_id IS ?2 AND name=?3 COLLATE NOCASE AND trash_id IS NULL)",params![project,parent,restored],|r|r.get(0)).map_err(storage)?
            } else {
                db.query_row("SELECT EXISTS(SELECT 1 FROM assets a LEFT JOIN asset_placements p ON p.asset_id=a.id WHERE a.project_id=?1 AND p.folder_id IS ?2 AND a.name=?3 COLLATE NOCASE AND a.trash_id IS NULL)",params![project,parent,restored],|r|r.get(0)).map_err(storage)?
            };
            if !collision {
                break;
            }
            restored = format!(
                "{} (restored {})",
                name.chars().take(90).collect::<String>(),
                index
            );
            index += 1;
        }
        let tx = db.transaction().map_err(storage)?;
        if kind == "folder" {
            tx.execute(
                "UPDATE asset_folders SET parent_id=?1,name=?2 WHERE id=?3",
                params![parent, restored, root],
            )
            .map_err(storage)?;
        } else {
            tx.execute(
                "UPDATE assets SET name=?1 WHERE id=?2",
                params![restored, root],
            )
            .map_err(storage)?;
            tx.execute("UPDATE asset_placements SET folder_id=?1,slot=CASE WHEN ?1 IS NULL OR EXISTS(SELECT 1 FROM asset_placements other JOIN assets a ON a.id=other.asset_id WHERE other.folder_id IS ?1 AND other.slot=asset_placements.slot AND a.trash_id IS NULL) THEN NULL ELSE slot END WHERE asset_id=?2",params![parent,root]).map_err(storage)?;
        }
        tx.execute("UPDATE asset_placements SET active=1 WHERE asset_id IN(SELECT id FROM assets WHERE trash_id=?1)",[id]).map_err(storage)?;
        tx.execute("UPDATE assets SET trash_id=NULL WHERE trash_id=?1", [id])
            .map_err(storage)?;
        tx.execute(
            "UPDATE asset_folders SET trash_id=NULL WHERE trash_id=?1",
            [id],
        )
        .map_err(storage)?;
        tx.execute("DELETE FROM trash_entries WHERE id=?1", [id])
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(restored)
    }
    pub fn purge_trash(&self, id: &str) -> Result<(), Error> {
        let gate = self.content_gate();
        let _guard = gate.lock();
        let mut db = self.connection()?;
        let project: String = db
            .query_row(
                "SELECT project_id FROM trash_entries WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        self.editable_project(&project)?;
        let mut q=db.prepare("SELECT hash FROM assets WHERE trash_id=?1 UNION SELECT v.hash FROM asset_versions v JOIN assets a ON a.id=v.asset_id WHERE a.trash_id=?1 UNION SELECT cover_hash FROM asset_folders WHERE trash_id=?1 AND cover_hash IS NOT NULL").map_err(storage)?;
        let hashes = q
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        drop(q);
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        tx.execute(
            "DELETE FROM asset_versions WHERE asset_id IN(SELECT id FROM assets WHERE trash_id=?1)",
            [id],
        )
        .map_err(storage)?;
        tx.execute("DELETE FROM asset_placements WHERE asset_id IN(SELECT id FROM assets WHERE trash_id=?1)",[id]).map_err(storage)?;
        tx.execute("DELETE FROM assets WHERE trash_id=?1", [id])
            .map_err(storage)?;
        tx.execute("UPDATE asset_folders SET parent_id=NULL WHERE trash_id IS NOT ?1 AND parent_id IN(SELECT id FROM asset_folders WHERE trash_id=?1)",[id]).map_err(storage)?;
        tx.execute("DELETE FROM asset_folders WHERE trash_id=?1", [id])
            .map_err(storage)?;
        tx.execute("DELETE FROM trash_entries WHERE id=?1", [id])
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
        self.collect_content(&hashes)
    }
    pub(super) fn collect_content(&self, hashes: &[String]) -> Result<(), Error> {
        let gate = self.content_gate();
        let _guard = gate.lock();
        let mut db = self.connection()?;
        for hash in hashes {
            if !apex_core::valid_hash(hash) {
                continue;
            }
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(storage)?;
            let used:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM assets WHERE hash=?1 UNION ALL SELECT 1 FROM asset_versions WHERE hash=?1 UNION ALL SELECT 1 FROM asset_folders WHERE cover_hash=?1 UNION ALL SELECT 1 FROM note_images WHERE hash=?1 UNION ALL SELECT 1 FROM publishing_media WHERE hash=?1 UNION ALL SELECT 1 FROM content_leases WHERE hash=?1 AND expires_at>?2 UNION ALL SELECT 1 FROM upload_jobs j WHERE j.hash=?1 AND (j.state IN ('submitting','polling') OR (j.state NOT IN ('complete','cancelled') AND EXISTS(SELECT 1 FROM upload_batch_items i JOIN upload_batches b ON b.id=i.batch_id WHERE i.job_id=j.id AND i.enabled=1 AND b.state IN ('running','paused')))))",params![hash,super::queue_now()],|r|r.get(0)).map_err(storage)?;
            if used {
                continue;
            }
            tx.execute("DELETE FROM content_leases WHERE hash=?1", [hash])
                .map_err(storage)?;
            tx.execute("DELETE FROM image_validation WHERE hash=?1", [hash])
                .map_err(storage)?;
            tx.execute("DELETE FROM managed_files WHERE hash=?1", [hash])
                .map_err(storage)?;
            tx.commit().map_err(storage)?;
            match std::fs::remove_file(self.root.join("files").join(hash)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(storage(e)),
            };
            let _ = std::fs::remove_file(self.root.join("thumbnails").join(format!("{hash}.png")));
        }
        Ok(())
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Workspace, String, String, String) {
        let d = tempfile::tempdir().unwrap();
        let ws = Workspace::open(d.path().join("ws")).unwrap();
        let p = ws
            .save_project(
                None,
                apex_core::ProjectInput {
                    name: "Trash".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let parent = ws.save_folder(&p.id, None, None, "Parent").unwrap();
        let v = apex_core::vehicle_catalogue().unwrap().vehicles.remove(0);
        let file = d.path().join("Frontplate.png");
        image::RgbImage::new(2, 2).save(&file).unwrap();
        let l = ws
            .import_livery(
                &p.id,
                Some(&parent.id),
                None,
                "Livery",
                &v.id,
                &[(file, "Frontplate.png".into(), Some("Frontplate 1".into()))],
                Some(0),
            )
            .unwrap();
        (d, ws, p.id, parent.id, l)
    }
    #[test]
    fn recursive_trash_restores_versions_and_resolves_name_collisions() {
        let (_d, ws, p, parent, l) = fixture();
        let asset = ws.assets(Some(&p)).unwrap().remove(0);
        let before = ws.versions(&asset.id).unwrap().len();
        let preview = ws.trash_preview(&parent, true).unwrap();
        assert_eq!(preview.folders, 2);
        assert_eq!(preview.files, 1);
        let t = ws.trash_item(&parent, true).unwrap();
        assert!(ws.assets(Some(&p)).unwrap().is_empty());
        assert!(ws.roblox_liveries("123").unwrap().is_empty());
        assert!(ws.folder(&l).is_err());
        ws.save_folder(&p, None, None, "Parent").unwrap();
        assert!(ws.restore_trash(&t).unwrap().contains("restored"));
        assert_eq!(ws.versions(&asset.id).unwrap().len(), before);
        assert_eq!(
            ws.livery_uploads(&l, "123").unwrap().textures[0].side,
            "Frontplate 1"
        );
    }
    #[test]
    fn deleting_parent_of_an_already_trashed_child_restores_child_at_root() {
        let (_d, ws, _p, parent, l) = fixture();
        let child = ws.trash_item(&l, true).unwrap();
        let par = ws.trash_item(&parent, true).unwrap();
        ws.purge_trash(&par).unwrap();
        ws.restore_trash(&child).unwrap();
        assert!(ws.folder(&l).unwrap().parent_id.is_none());
    }
    #[test]
    fn shared_blobs_and_saved_post_images_survive_purge_and_history_is_kept() {
        let (d, ws, p, _parent, l) = fixture();
        let a = ws.assets(Some(&p)).unwrap().remove(0);
        let spare = ws.save_folder(&p, None, None, "Spare").unwrap();
        ws.copy_item(&p, &a.id, false, Some(&spare.id)).unwrap();
        ws.create_upload_batch("batch", "123", std::slice::from_ref(&l), false)
            .unwrap();
        let t = ws.trash_item(&l, true).unwrap();
        assert!(ws.lease_upload_tasks(false, 5).unwrap().is_empty());
        ws.purge_trash(&t).unwrap();
        assert!(ws.root.join("files").join(&a.hash).exists());
        assert_eq!(ws.upload_snapshot().unwrap().jobs[0].state, "cancelled");
        let backup = d.path().join("trash.apexbackup");
        ws.backup(&backup).unwrap();
        let other = Workspace::open(d.path().join("restore")).unwrap();
        other.restore_backup(&backup).unwrap();
        assert_eq!(other.upload_snapshot().unwrap().jobs.len(), 1);
    }
    #[test]
    fn unreferenced_blobs_are_removed_but_leased_content_is_kept() {
        let (_d, ws, p, _parent, l) = fixture();
        let a = ws.assets(Some(&p)).unwrap().remove(0);
        ws.pin_content("preview", std::slice::from_ref(&a.hash))
            .unwrap();
        let t = ws.trash_item(&l, true).unwrap();
        ws.purge_trash(&t).unwrap();
        assert!(ws.root.join("files").join(&a.hash).exists());
        ws.release_content("preview").unwrap();
        ws.collect_content(std::slice::from_ref(&a.hash)).unwrap();
        assert!(!ws.root.join("files").join(&a.hash).exists());
    }
    #[test]
    fn backup_restores_trash_and_archived_projects_remain_read_only() {
        let (d, ws, p, parent, _l) = fixture();
        let t = ws.trash_item(&parent, true).unwrap();
        let file = d.path().join("backup.apexbackup");
        ws.backup(&file).unwrap();
        let restored = Workspace::open(d.path().join("restored")).unwrap();
        restored.restore_backup(&file).unwrap();
        assert_eq!(restored.trash_items().unwrap().len(), 1);
        restored.archive_project(&p, true).unwrap();
        assert!(restored.restore_trash(&t).is_err());
        assert!(restored.purge_trash(&t).is_err());
        restored.archive_project(&p, false).unwrap();
        restored.restore_trash(&t).unwrap();
        assert_eq!(restored.assets(Some(&p)).unwrap().len(), 1);
    }
}
