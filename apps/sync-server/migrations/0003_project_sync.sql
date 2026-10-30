CREATE TABLE sync_records (
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    project_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN ('project', 'note', 'folder', 'livery', 'asset', 'placement', 'post', 'post_media')),
    record_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    data JSONB,
    deleted BOOLEAN NOT NULL DEFAULT FALSE,
    updated_by UUID NOT NULL REFERENCES users(id),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (team_id, project_id, record_type, record_id),
    CHECK ((deleted AND data IS NULL) OR (NOT deleted AND data IS NOT NULL)),
    CHECK (record_type <> 'project' OR project_id = record_id)
);

CREATE INDEX sync_records_project ON sync_records(team_id, project_id, record_type, record_id);

CREATE TABLE sync_events (
    cursor BIGSERIAL PRIMARY KEY,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    project_id UUID NOT NULL,
    record_type TEXT NOT NULL,
    record_id UUID NOT NULL,
    revision BIGINT NOT NULL,
    data JSONB,
    deleted BOOLEAN NOT NULL,
    actor_user_id UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX sync_events_after_cursor ON sync_events(team_id, project_id, cursor);

CREATE TABLE project_blobs (
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    project_id UUID NOT NULL,
    content_hash CHAR(64) NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    byte_size BIGINT NOT NULL CHECK (byte_size >= 0),
    uploaded_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (team_id, project_id, content_hash)
);
