-- User-defined groups for network subnets.

CREATE TABLE groups (
    id INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

ALTER TABLE networks ADD COLUMN group_id INTEGER REFERENCES groups (id) ON DELETE SET NULL;

CREATE INDEX idx_networks_group_id ON networks (group_id);
