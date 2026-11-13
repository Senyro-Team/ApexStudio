//! SQLite workspace adapter and immutable, content-addressed managed files.
use apex_core::{Error, WorkspaceInfo, WorkspaceRepository};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

mod assets;
mod markdown_presets;
mod media_library;
mod overlays;
pub use assets::{validate_png as validate_texture, validate_png_dimensions};
pub use markdown_presets::MarkdownPreset;
pub use media_library::{GeneratedPreview, MediaLibraryItem, MediaLibraryMetadata};
pub use overlays::OverlayTemplate;
mod notes;
mod packages;
mod projects;
pub use packages::PackagePreview;
mod publishing;
mod roblox;
pub use roblox::{DecalUpload, LiveryUploads, UploadTexture};
mod pipeline;
pub use pipeline::{png_dimensions, PreparationProgress, PreparedTexture};
mod queue;
pub use queue::*;
mod group_revenue;
mod posts;
mod statistics;
mod team_sync;
mod trash;
pub use group_revenue::*;
pub use posts::*;
pub use statistics::*;
pub use team_sync::*;
pub use trash::*;
const SCHEMA_VERSION: u32 = 26;

fn storage(error: impl std::fmt::Display) -> Error {
    Error::Storage(error.to_string())
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, Error> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(storage)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(storage)?;
    for name in columns {
        if name.map_err(storage)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_locking_protocol(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::FileLockingProtocolFailed)
}

/// One active workspace. Operations use short-lived, identically configured connections.
#[derive(Clone)]
pub struct Workspace {
    root: PathBuf,
    // Keep the WAL open between operations, avoiding repeated last-connection
    // cleanup and recovery while other threads are opening connections.
    anchor: Arc<Mutex<Option<Connection>>>,
}

impl Workspace {
    /// Create the managed layout and migrate atomically. Existing files are never removed.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_with_progress(root, |_| {})
    }

    /// Open a workspace while reporting long-running initialization work.
    /// The caller is responsible for running this on a worker thread.
    pub fn open_with_progress(
        root: impl AsRef<Path>,
        mut progress: impl FnMut(&str),
    ) -> Result<Self, Error> {
        progress("Opening workspace…");
        fs::create_dir_all(root.as_ref()).map_err(storage)?;
        let root = fs::canonicalize(root.as_ref()).map_err(storage)?;
        let workspace = Self {
            root,
            anchor: Arc::new(Mutex::new(None)),
        };
        for directory in ["files", "staging", "backups", "thumbnails"] {
            fs::create_dir_all(workspace.root.join(directory)).map_err(storage)?;
        }
        // Serialize migrations across desktop/CLI processes. The lock is held
        // only during initialization; normal workspace operations stay concurrent.
        let migration_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(workspace.root.join(".workspace-migration.lock"))
            .map_err(storage)?;
        migration_lock.lock().map_err(storage)?;
        let mut connection = workspace.connection()?;
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(storage)?;
        if version > SCHEMA_VERSION {
            return Err(Error::NewerSchema);
        }
        if version > 0 && version < SCHEMA_VERSION {
            progress("Creating a safety backup before workspace updates…");
            workspace.backup(&workspace.root.join("backups").join(format!(
                "before-schema-update-{}.apexbackup",
                uuid::Uuid::now_v7()
            )))?;
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
        if version < 4 {
            progress("Updating workspace schema…");
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
            progress("Updating workspace schema…");
            connection.execute_batch("BEGIN IMMEDIATE;CREATE TABLE publishing_drafts(project_id TEXT PRIMARY KEY REFERENCES projects(id),data TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 1);CREATE TABLE publishing_media(project_id TEXT NOT NULL REFERENCES publishing_drafts(project_id),hash TEXT NOT NULL REFERENCES managed_files(hash),name TEXT NOT NULL,ordinal INTEGER NOT NULL,PRIMARY KEY(project_id,hash));PRAGMA user_version=5;COMMIT;").map_err(storage)?;
        }
        if version < 6 {
            connection.execute_batch("BEGIN IMMEDIATE; CREATE TABLE roblox_uploads(account_id TEXT NOT NULL,hash TEXT NOT NULL REFERENCES managed_files(hash),state TEXT NOT NULL,operation TEXT,asset_id TEXT,moderation TEXT NOT NULL DEFAULT 'unknown',message TEXT NOT NULL DEFAULT '',PRIMARY KEY(account_id,hash)); PRAGMA user_version=6; COMMIT;").map_err(storage)?;
        }
        if version < 7 {
            connection
                .execute_batch(include_str!("schema7.sql"))
                .map_err(storage)?;
        }
        if version < 8 {
            connection
                .execute_batch(include_str!("schema8.sql"))
                .map_err(storage)?;
            let issues: Option<String> = connection
                .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
                .optional()
                .map_err(storage)?;
            if issues.is_some() {
                return Err(storage("Migration foreign-key check failed."));
            }
        }
        if version < 9 {
            connection
                .execute_batch(include_str!("schema9.sql"))
                .map_err(storage)?;
        }
        if version < 10 {
            connection
                .execute_batch(include_str!("schema10.sql"))
                .map_err(storage)?;
        }
        if version < 11 {
            connection
                .execute_batch(include_str!("schema11.sql"))
                .map_err(storage)?;
        }
        if version < 12 {
            connection
                .execute_batch(include_str!("schema12.sql"))
                .map_err(storage)?;
        }
        if version < 13 {
            connection
                .execute_batch(include_str!("schema13.sql"))
                .map_err(storage)?;
        }
        if version < 14 {
            connection
                .execute_batch(include_str!("schema14.sql"))
                .map_err(storage)?;
        }
        if version < 15 {
            connection
                .execute_batch(include_str!("schema15.sql"))
                .map_err(storage)?;
        }
        if version < 16 {
            progress("Updating workspace schema…");
            connection
                .execute_batch(include_str!("schema16.sql"))
                .map_err(storage)?;
        }
        if version < 17 {
            progress("Updating workspace schema…");
            connection
                .execute_batch(include_str!("schema17.sql"))
                .map_err(storage)?;
        }
        if version < 18 {
            progress("Updating workspace schema…");
            let transaction = connection.transaction().map_err(storage)?;
            for (table, column, definition) in [
                (
                    "roblox_group_revenue_groups",
                    "selected_pass_ids_json",
                    "TEXT NOT NULL DEFAULT '[]'",
                ),
                ("roblox_group_revenue_groups", "api_updated_at", "TEXT"),
                ("roblox_group_revenue_groups", "api_coverage_from", "TEXT"),
                ("roblox_group_revenue_groups", "api_coverage_to", "TEXT"),
                ("roblox_group_revenue_groups", "api_summary_json", "TEXT"),
                (
                    "roblox_group_revenue_groups",
                    "api_status",
                    "TEXT NOT NULL DEFAULT 'never'",
                ),
                (
                    "roblox_group_revenue_sales",
                    "external_transaction_id",
                    "TEXT",
                ),
                (
                    "roblox_group_revenue_sales",
                    "source",
                    "TEXT NOT NULL DEFAULT 'csv'",
                ),
                (
                    "roblox_group_revenue_sales",
                    "dedupe_confidence",
                    "TEXT NOT NULL DEFAULT 'exact_fields'",
                ),
            ] {
                if !table_has_column(&transaction, table, column)? {
                    transaction
                        .execute_batch(&format!(
                            "ALTER TABLE {table} ADD COLUMN {column} {definition};"
                        ))
                        .map_err(storage)?;
                }
            }
            transaction
                .execute_batch(include_str!("schema18.sql"))
                .map_err(storage)?;
            transaction
                .pragma_update(None, "user_version", 18)
                .map_err(storage)?;
            transaction.commit().map_err(storage)?;
        }
        if version < 19 {
            progress("Updating workspace schema…");
            let transaction = connection.transaction().map_err(storage)?;
            transaction
                .execute_batch(include_str!("schema19.sql"))
                .map_err(storage)?;
            transaction
                .pragma_update(None, "user_version", 19)
                .map_err(storage)?;
            transaction.commit().map_err(storage)?;
        }
        if version < 20 {
            connection
                .execute_batch(include_str!("schema20.sql"))
                .map_err(storage)?;
            connection
                .pragma_update(None, "user_version", 20)
                .map_err(storage)?;
        }
        if version < 21 {
            connection
                .execute_batch(include_str!("schema21.sql"))
                .map_err(storage)?;
            connection
                .pragma_update(None, "user_version", 21)
                .map_err(storage)?;
        }
        if version < 22 {
            connection
                .execute_batch(include_str!("schema22.sql"))
                .map_err(storage)?;
            connection
                .pragma_update(None, "user_version", 22)
                .map_err(storage)?;
        }
        if version < 23 {
            connection
                .execute_batch(include_str!("schema23.sql"))
                .map_err(storage)?;
        }
        if version < 24 {
            connection
                .execute_batch(include_str!("schema24.sql"))
                .map_err(storage)?;
        }
        if version < 25 {
            connection
                .execute_batch(include_str!("schema25.sql"))
                .map_err(storage)?;
        }
        if version < 26 {
            connection
                .execute_batch(include_str!("schema26.sql"))
                .map_err(storage)?;
        }
        *workspace
            .anchor
            .lock()
            .map_err(|_| storage("Workspace connection unavailable."))? = Some(connection);
        Ok(workspace)
    }

    fn connection(&self) -> Result<Connection, Error> {
        let mut attempt = 0;
        loop {
            match self.open_connection() {
                Ok(connection) => return Ok(connection),
                Err(error) if is_locking_protocol(&error) && attempt < 3 => {
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(100 * attempt));
                }
                Err(error) => return Err(storage(error)),
            }
        }
    }

    fn open_connection(&self) -> rusqlite::Result<Connection> {
        let connection = Connection::open(self.root.join("workspace.sqlite3"))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        Ok(connection)
    }

    /// Stream a regular file into managed storage. Originals stay untouched.
    /// Duplicate bytes share a single blob. No browser-supplied paths reach this API yet.
    pub fn import_file(&self, source: &Path) -> Result<String, Error> {
        let gate = self.content_gate();
        let _content = gate.lock();
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
                    "INSERT INTO managed_files(hash,size,local_available) VALUES (?1,?2,1) ON CONFLICT(hash) DO UPDATE SET local_available=1,size=excluded.size",
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
    fn protocol_errors_are_distinguished_from_other_sqlite_failures() {
        let protocol = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_PROTOCOL),
            None,
        );
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        assert!(is_locking_protocol(&protocol));
        assert!(!is_locking_protocol(&busy));
        assert!(!is_locking_protocol(&rusqlite::Error::QueryReturnedNoRows));
        assert!(rusqlite::version_number() >= 3_051_003);
    }
    #[test]
    fn concurrent_workspace_reads_keep_the_wal_available() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(directory.path()).unwrap();
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let workspace = workspace.clone();
                std::thread::spawn(move || {
                    for _ in 0..30 {
                        assert_eq!(workspace.info().unwrap().name, "My workspace");
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
    }
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
