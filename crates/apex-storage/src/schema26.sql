BEGIN IMMEDIATE;
ALTER TABLE overlay_templates RENAME TO overlay_templates_before_26;

CREATE TABLE overlay_templates (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  width INTEGER NOT NULL CHECK(width = 1920),
  height INTEGER NOT NULL CHECK(height IN (1080, 1200)),
  layers_json TEXT NOT NULL CHECK(length(layers_json) <= 8388608),
  created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

INSERT INTO overlay_templates(id,name,width,height,layers_json,created_at,updated_at)
SELECT id,name,width,height,layers_json,created_at,updated_at
FROM overlay_templates_before_26;

DROP TABLE overlay_templates_before_26;
CREATE INDEX overlay_templates_updated ON overlay_templates(updated_at DESC, name COLLATE NOCASE);
PRAGMA user_version = 26;
COMMIT;
