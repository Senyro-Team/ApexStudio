BEGIN IMMEDIATE;
ALTER TABLE asset_folders ADD COLUMN primary_color TEXT NOT NULL DEFAULT '#FFFFFF';
ALTER TABLE asset_folders ADD COLUMN secondary_color TEXT;
PRAGMA user_version = 27;
COMMIT;
