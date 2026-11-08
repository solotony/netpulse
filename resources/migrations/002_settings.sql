-- Application settings key-value store (autostart, background mode, etc.).

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
