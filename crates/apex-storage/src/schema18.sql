CREATE UNIQUE INDEX IF NOT EXISTS roblox_group_revenue_external_id
  ON roblox_group_revenue_sales(group_id, external_transaction_id)
  WHERE external_transaction_id IS NOT NULL;
CREATE TABLE IF NOT EXISTS roblox_group_revenue_passes (
  group_id TEXT NOT NULL REFERENCES roblox_group_revenue_groups(group_id) ON DELETE CASCADE,
  asset_id TEXT NOT NULL,
  universe_id TEXT NOT NULL,
  name TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'catalog',
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  PRIMARY KEY(group_id, asset_id)
);
CREATE INDEX IF NOT EXISTS roblox_group_revenue_passes_universe
  ON roblox_group_revenue_passes(group_id, universe_id);
