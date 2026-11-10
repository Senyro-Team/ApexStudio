BEGIN IMMEDIATE;
CREATE TABLE overlay_render_versions (
  version_group_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK(version > 0),
  hash TEXT NOT NULL REFERENCES managed_files(hash),
  name TEXT NOT NULL,
  target TEXT NOT NULL CHECK(target IN ('erlcx', 'execlab')),
  composition_json TEXT NOT NULL CHECK(json_valid(composition_json) AND length(composition_json) <= 524288),
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  PRIMARY KEY(version_group_id, version),
  UNIQUE(version_group_id, hash)
);
CREATE INDEX overlay_render_versions_hash ON overlay_render_versions(hash, created_at DESC);
PRAGMA user_version = 25;
COMMIT;
