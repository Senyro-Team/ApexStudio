//! SQLite workspace adapter and immutable, content-addressed managed files.
use apex_core::{Error, WorkspaceInfo, WorkspaceRepository};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

mod assets;
pub use assets::validate_png as validate_texture;
mod notes;
mod packages;
mod projects;
pub use packages::PackagePreview;
mod publishing;
mod roblox;
pub use roblox::{DecalUpload, LiveryUploads, UploadTexture};
const SCHEMA_VERSION: u32 = 6;

fn storage(error: impl std::fmt::Display) -> Error {
    Error::Storage(error.to_string())
}

/// One active workspace. Connections are short-lived and configured identically.
#[derive(Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// Create the managed layout and migrate atomically. Existing files are never removed.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, Error> {
        fs::create_dir_all(root.as_ref()).map_err(storage)?;
        let root = fs::canonicalize(root.as_ref()).map_err(storage)?;
        let workspace = Self { root };
        let mut connection = workspace.connection()?;
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(storage)?;
        if version > SCHEMA_VERSION {
            return Err(Error::NewerSchema);
        }
        if version == 0 {
            let transaction = connection.transaction().map_err(storage)?;
            transaction.execute_batch("CREATE TABLE IF NOT EXISTS workspace_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                INSERT OR IGNORE INTO workspace_settings VALUES ('name', 'My workspace');
                CREATE TABLE IF NOT EXISTS managed_files (hash TEXT PRIMARY KEY, size INTEGER NOT NULL, imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
                PRAGMA user_version = 1;").map_err(storage)?;
            transaction.commit().map_err(storage)?;
        }
        if version < 2 {
            let transaction = connection.transaction().map_err(storage)?;
            transaction
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY, metadata TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1)));
                CREATE INDEX IF NOT EXISTS projects_recent ON projects(archived,updated_at DESC);
                CREATE TABLE IF NOT EXISTS assets (
                id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id),
                name TEXT NOT NULL, hash TEXT NOT NULL REFERENCES managed_files(hash),
                imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                UNIQUE(project_id,name,hash));
                CREATE INDEX IF NOT EXISTS assets_project ON assets(project_id);
                PRAGMA user_version=2;",
                )
                .map_err(storage)?;
            transaction.commit().map_err(storage)?;
        }
        if version < 3 {
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE notes (id TEXT PRIMARY KEY, project_id TEXT REFERENCES projects(id), title TEXT NOT NULL, document TEXT NOT NULL, markdown TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 1, archived INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
                CREATE INDEX notes_project ON notes(project_id,archived,updated_at DESC);
                CREATE TABLE note_images(note_id TEXT NOT NULL REFERENCES notes(id),hash TEXT NOT NULL REFERENCES managed_files(hash),PRIMARY KEY(note_id,hash));
                PRAGMA user_version=3; COMMIT;").map_err(storage)?;
        }
        for directory in ["files", "staging", "backups", "thumbnails"] {
            fs::create_dir_all(workspace.root.join(directory)).map_err(storage)?;
        }
        if version < 4 {
            if version > 0 {
                workspace.backup(&workspace.root.join("backups").join(format!(
                    "before-schema-4-{}.apexbackup",
                    uuid::Uuid::now_v7()
                )))?;
            }
            connection.execute_batch("BEGIN IMMEDIATE;
              DROP INDEX assets_project;
              ALTER TABLE assets RENAME TO assets_v3;
              CREATE TABLE assets(id TEXT PRIMARY KEY,project_id TEXT NOT NULL REFERENCES projects(id),name TEXT NOT NULL,hash TEXT NOT NULL REFERENCES managed_files(hash),imported_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')));
              INSERT INTO assets SELECT * FROM assets_v3; DROP TABLE assets_v3;
              CREATE INDEX assets_project ON assets(project_id);
              CREATE TABLE asset_folders(id TEXT PRIMARY KEY,project_id TEXT NOT NULL REFERENCES projects(id),parent_id TEXT REFERENCES asset_folders(id),name TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('folder','livery')),vehicle_id TEXT,cover_hash TEXT REFERENCES managed_files(hash));
              CREATE INDEX folders_project ON asset_folders(project_id,parent_id);
              CREATE UNIQUE INDEX folder_names ON asset_folders(project_id,COALESCE(parent_id,''),name COLLATE NOCASE);
              CREATE TABLE asset_placements(asset_id TEXT PRIMARY KEY REFERENCES assets(id),folder_id TEXT REFERENCES asset_folders(id),slot TEXT);
              CREATE UNIQUE INDEX livery_slots ON asset_placements(folder_id,slot) WHERE slot IS NOT NULL;
              CREATE TABLE asset_versions(id TEXT PRIMARY KEY,asset_id TEXT NOT NULL REFERENCES assets(id),hash TEXT NOT NULL REFERENCES managed_files(hash),name TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),UNIQUE(asset_id,hash,name));
              PRAGMA user_version=4; COMMIT;").map_err(storage)?;
        }
        if version < 5 {
            if version > 0 {
                workspace.backup(&workspace.root.join("backups").join(format!(
                    "before-schema-5-{}.apexbackup",
                    uuid::Uuid::now_v7()
                )))?;
            }
            connection.execute_batch("BEGIN IMMEDIATE;CREATE TABLE publishing_drafts(project_id TEXT PRIMARY KEY REFERENCES projects(id),data TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 1);CREATE TABLE publishing_media(project_id TEXT NOT NULL REFERENCES publishing_drafts(project_id),hash TEXT NOT NULL REFERENCES managed_files(hash),name TEXT NOT NULL,ordinal INTEGER NOT NULL,PRIMARY KEY(project_id,hash));PRAGMA user_version=5;COMMIT;").map_err(storage)?;
        }
        if version < 6 {
            if version > 0 {
                workspace.backup(&workspace.root.join("backups").join(format!(
                    "before-schema-6-{}.apexbackup",
                    uuid::Uuid::now_v7()
                )))?;
            }
            connection.execute_batch("BEGIN IMMEDIATE; CREATE TABLE roblox_uploads(account_id TEXT NOT NULL,hash TEXT NOT NULL REFERENCES managed_files(hash),state TEXT NOT NULL,operation TEXT,asset_id TEXT,moderation TEXT NOT NULL DEFAULT 'unknown',message TEXT NOT NULL DEFAULT '',PRIMARY KEY(account_id,hash)); PRAGMA user_version=6; COMMIT;").map_err(storage)?;
        }
        Ok(workspace)
    }

    fn connection(&self) -> Result<Connection, Error> {
        let connection = Connection::open(self.root.join("workspace.sqlite3")).map_err(storage)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(storage)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
            )
            .map_err(storage)?;
        Ok(connection)
    }

    /// Stream a regular file into managed storage. Originals stay untouched.
    /// Duplicate bytes share a single blob. No browser-supplied paths reach this API yet.
    pub fn import_file(&self, source: &Path) -> Result<String, Error> {
        let metadata = fs::symlink_metadata(source).map_err(storage)?;
        if !metadata.file_type().is_file() {
            return Err(storage("Select a regular file."));
        }
        let temporary = self
            .root
            .join("staging")
            .join(uuid::Uuid::now_v7().to_string());
        let result = (|| {
            let mut input = fs::File::open(source).map_err(storage)?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(storage)?;
            let mut hasher = Sha256::new();
            let mut size: u64 = 0;
            let mut buffer = [0; 64 * 1024];
            loop {
                let count = input.read(&mut buffer).map_err(storage)?;
                if count == 0 {
                    break;
                }
                output.write_all(&buffer[..count]).map_err(storage)?;
                hasher.update(&buffer[..count]);
                size += count as u64;
            }
            output.sync_all().map_err(storage)?;
            drop(output);
            let database_size = i64::try_from(size).map_err(storage)?;
            let hash = format!("{:x}", hasher.finalize());
            let target = self.root.join("files").join(&hash);
            // A hard link publishes the complete staged file without overwriting an existing blob.
            match fs::hard_link(&temporary, &target) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(storage(error)),
            }
            self.connection()?
                .execute(
                    "INSERT OR IGNORE INTO managed_files(hash,size) VALUES (?1,?2)",
                    params![hash, database_size],
                )
                .map_err(storage)?;
            Ok(hash)
        })();
        let _ = fs::remove_file(&temporary);
        result
    }
}

impl WorkspaceRepository for Workspace {
    fn info(&self) -> Result<WorkspaceInfo, Error> {
        let connection = self.connection()?;
        let name = connection
            .query_row(
                "SELECT value FROM workspace_settings WHERE key='name'",
                [],
                |row| row.get(0),
            )
            .map_err(storage)?;
        let count: i64 = connection
            .query_row("SELECT count(*) FROM managed_files", [], |row| row.get(0))
            .map_err(storage)?;
        Ok(WorkspaceInfo {
            name,
            root: self.root.display().to_string(),
            schema_version: SCHEMA_VERSION,
            managed_file_count: u64::try_from(count).map_err(storage)?,
        })
    }
    fn rename(&self, name: &str) -> Result<WorkspaceInfo, Error> {
        let name = apex_core::workspace_name(name)?;
        self.connection()?
            .execute(
                "UPDATE workspace_settings SET value=?1 WHERE key='name'",
                [name],
            )
            .map_err(storage)?;
        self.info()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn settings_survive_reopen_and_migration_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(directory.path()).unwrap();
        apex_core::rename_workspace(&workspace, "Apex Designs").unwrap();
        assert_eq!(
            Workspace::open(directory.path())
                .unwrap()
                .info()
                .unwrap()
                .name,
            "Apex Designs"
        );
    }
    #[test]
    fn import_preserves_source_and_deduplicates_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("original.txt");
        fs::write(&source, b"original content").unwrap();
        let workspace = Workspace::open(directory.path().join("workspace")).unwrap();
        let hash = workspace.import_file(&source).unwrap();
        assert_eq!(workspace.import_file(&source).unwrap(), hash);
        assert_eq!(fs::read(source).unwrap(), b"original content");
        assert_eq!(
            fs::read(workspace.root.join("files").join(hash)).unwrap(),
            b"original content"
        );
        assert_eq!(workspace.info().unwrap().managed_file_count, 1);
        assert_eq!(
            fs::read_dir(workspace.root.join("staging"))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn future_schema_is_rejected_without_downgrade() {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join("workspace.sqlite3")).unwrap();
        connection.pragma_update(None, "user_version", 999).unwrap();
        assert!(matches!(
            Workspace::open(directory.path()),
            Err(Error::NewerSchema)
        ));
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 999);
    }
}
