CREATE TABLE server_instance (
    id SMALLINT PRIMARY KEY CHECK (id = 1),
    instance_id UUID NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
