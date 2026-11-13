ALTER TABLE media_library ADD COLUMN provider_image_id TEXT;
CREATE UNIQUE INDEX media_library_provider_image_id
  ON media_library(provider_image_id) WHERE provider_image_id IS NOT NULL;
