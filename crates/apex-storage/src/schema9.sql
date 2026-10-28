BEGIN IMMEDIATE;
CREATE TABLE posts(id TEXT PRIMARY KEY,data TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 1,created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),updated_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),legacy_project_id TEXT UNIQUE);
CREATE TABLE post_media(post_id TEXT NOT NULL REFERENCES posts(id),hash TEXT NOT NULL REFERENCES managed_files(hash),name TEXT NOT NULL,ordinal INTEGER NOT NULL,PRIMARY KEY(post_id,hash));
INSERT INTO posts(id,data,revision,legacy_project_id) SELECT project_id,data,revision,project_id FROM publishing_drafts;
INSERT INTO post_media SELECT project_id,hash,name,ordinal FROM publishing_media;
PRAGMA user_version=9;
COMMIT;
