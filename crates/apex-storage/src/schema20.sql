CREATE TABLE IF NOT EXISTS post_markdown_media(
  post_id TEXT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
  hash TEXT NOT NULL REFERENCES managed_files(hash),
  PRIMARY KEY(post_id,hash)
);
CREATE INDEX IF NOT EXISTS post_markdown_media_hash ON post_markdown_media(hash);
