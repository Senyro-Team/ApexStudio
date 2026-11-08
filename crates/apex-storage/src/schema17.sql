BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS roblox_group_revenue_groups (
  group_id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  experience_ids_json TEXT NOT NULL,
  pass_mappings_json TEXT NOT NULL,
  last_import_at TEXT,
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE IF NOT EXISTS roblox_group_revenue_sales (
  group_id TEXT NOT NULL REFERENCES roblox_group_revenue_groups(group_id) ON DELETE CASCADE,
  sale_fingerprint TEXT NOT NULL,
  universe_id TEXT NOT NULL,
  asset_id TEXT NOT NULL,
  sale_at TEXT NOT NULL,
  sale_location TEXT NOT NULL,
  hold_status TEXT NOT NULL CHECK(hold_status IN ('Released','Held','Cancelled')),
  price INTEGER NOT NULL CHECK(price >= 0),
  revenue INTEGER NOT NULL CHECK(revenue >= 0),
  import_batch_id TEXT NOT NULL,
  imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  PRIMARY KEY(group_id, sale_fingerprint)
);
CREATE INDEX IF NOT EXISTS roblox_group_revenue_date ON roblox_group_revenue_sales(group_id, sale_at);
PRAGMA user_version=17;
COMMIT;
