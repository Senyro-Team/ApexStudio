BEGIN IMMEDIATE;
CREATE TABLE markdown_presets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 100),
    kind TEXT NOT NULL CHECK(kind IN ('text', 'image', 'invite')),
    block_json TEXT NOT NULL CHECK(json_valid(block_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX markdown_presets_recent ON markdown_presets(updated_at DESC, name COLLATE NOCASE);
PRAGMA user_version = 23;
COMMIT;
