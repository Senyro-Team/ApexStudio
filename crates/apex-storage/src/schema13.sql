BEGIN IMMEDIATE;
ALTER TABLE upload_jobs ADD COLUMN passes_required INTEGER NOT NULL DEFAULT 2 CHECK(passes_required IN (1,2));
INSERT OR IGNORE INTO workspace_settings(key,value) VALUES('roblox_double_upload','true');
PRAGMA user_version=13;
COMMIT;
