CREATE TABLE team_decal_codes (
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    content_hash CHAR(64) NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    decal_asset_id VARCHAR(20) NOT NULL CHECK (decal_asset_id ~ '^[1-9][0-9]{0,19}$'),
    uploaded_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (team_id, content_hash)
);
