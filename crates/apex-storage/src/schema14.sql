BEGIN IMMEDIATE;
ALTER TABLE managed_files ADD COLUMN local_available INTEGER NOT NULL DEFAULT 1 CHECK(local_available IN (0,1));
PRAGMA user_version=14;
COMMIT;
