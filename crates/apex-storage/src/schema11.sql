BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS statistics_sales (
  id TEXT PRIMARY KEY,
  profile_id TEXT NOT NULL,
  collected_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  data_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS statistics_sales_profile_time ON statistics_sales(profile_id, collected_at, id);
PRAGMA user_version=11;
COMMIT;
