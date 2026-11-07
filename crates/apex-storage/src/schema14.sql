BEGIN IMMEDIATE;
ALTER TABLE project_sync ADD COLUMN received_from_team INTEGER NOT NULL DEFAULT 0 CHECK(received_from_team IN (0,1));
PRAGMA user_version=14;
COMMIT;
