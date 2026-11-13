CREATE TABLE IF NOT EXISTS media_library (
  id TEXT PRIMARY KEY,
  hash TEXT REFERENCES managed_files(hash),
  name TEXT NOT NULL,
  alt_text TEXT NOT NULL DEFAULT '',
  image_url TEXT NOT NULL DEFAULT '',
  default_link_url TEXT,
  owner_scope TEXT NOT NULL DEFAULT 'personal' CHECK (owner_scope IN ('personal', 'team')),
  team_id TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  CHECK ((owner_scope = 'personal' AND team_id IS NULL) OR (owner_scope = 'team' AND team_id IS NOT NULL)),
  CHECK (hash IS NOT NULL OR image_url <> '')
);
CREATE UNIQUE INDEX IF NOT EXISTS media_library_by_hash
  ON media_library(hash) WHERE hash IS NOT NULL;
CREATE INDEX IF NOT EXISTS media_library_recent
  ON media_library(owner_scope, team_id, updated_at DESC);
