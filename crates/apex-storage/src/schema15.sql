BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS shared_decal_codes (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  team_id TEXT NOT NULL,
  hash TEXT NOT NULL REFERENCES managed_files(hash),
  decal_asset_id TEXT NOT NULL,
  PRIMARY KEY(project_id,hash)
);
PRAGMA user_version=15;
COMMIT;
