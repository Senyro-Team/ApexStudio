BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS project_sync (
  project_id TEXT PRIMARY KEY,
  origin TEXT NOT NULL,
  server_instance TEXT NOT NULL,
  team_id TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  local_hash TEXT,
  revision INTEGER NOT NULL DEFAULT 0,
  cursor INTEGER NOT NULL DEFAULT 0,
  message TEXT NOT NULL DEFAULT 'Waiting to sync',
  synced_at TEXT
);
PRAGMA user_version=12;
COMMIT;
