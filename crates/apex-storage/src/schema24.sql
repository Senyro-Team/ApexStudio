BEGIN IMMEDIATE;
ALTER TABLE media_library ADD COLUMN folder TEXT NOT NULL DEFAULT '' CHECK(length(folder) <= 80);
ALTER TABLE media_library ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(tags_json));
PRAGMA user_version = 24;
COMMIT;
