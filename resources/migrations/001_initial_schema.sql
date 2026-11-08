-- Initial Network Monitor schema (task.md section 8).
-- Enrichment lives in dedicated tables and is joined, not denormalized onto connections.

CREATE TABLE processes (
    id INTEGER PRIMARY KEY NOT NULL,
    pid INTEGER NOT NULL,
    process_name TEXT,
    executable_path TEXT,
    process_start_time TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    UNIQUE (pid, process_start_time)
);

CREATE TABLE connections (
    id INTEGER PRIMARY KEY NOT NULL,
    process_id INTEGER NOT NULL REFERENCES processes (id) ON DELETE CASCADE,
    protocol TEXT NOT NULL,
    direction TEXT NOT NULL,
    local_ip TEXT NOT NULL,
    local_port INTEGER NOT NULL,
    remote_ip TEXT NOT NULL,
    remote_port INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    bytes_sent INTEGER,
    bytes_received INTEGER,
    collector_id TEXT
);

CREATE TABLE connection_events (
    id INTEGER PRIMARY KEY NOT NULL,
    connection_id INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    event_data TEXT
);

CREATE TABLE ip_addresses (
    id INTEGER PRIMARY KEY NOT NULL,
    ip TEXT NOT NULL UNIQUE,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);

CREATE TABLE domains (
    id INTEGER PRIMARY KEY NOT NULL,
    domain TEXT NOT NULL UNIQUE,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);

CREATE TABLE ip_domains (
    ip_id INTEGER NOT NULL REFERENCES ip_addresses (id) ON DELETE CASCADE,
    domain_id INTEGER NOT NULL REFERENCES domains (id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    confidence REAL,
    PRIMARY KEY (ip_id, domain_id, source)
);

CREATE TABLE networks (
    id INTEGER PRIMARY KEY NOT NULL,
    cidr TEXT NOT NULL UNIQUE,
    net_range TEXT,
    net_name TEXT,
    organization TEXT,
    asn INTEGER,
    country TEXT,
    source TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE ip_networks (
    ip_id INTEGER NOT NULL REFERENCES ip_addresses (id) ON DELETE CASCADE,
    network_id INTEGER NOT NULL REFERENCES networks (id) ON DELETE CASCADE,
    PRIMARY KEY (ip_id, network_id)
);

-- Indexes: timestamp/time, process, remote IP, domain, network, protocol
CREATE INDEX idx_processes_last_seen_at ON processes (last_seen_at);
CREATE INDEX idx_connections_started_at ON connections (started_at);
CREATE INDEX idx_connections_ended_at ON connections (ended_at);
CREATE INDEX idx_connections_process_id ON connections (process_id);
CREATE INDEX idx_connections_remote_ip ON connections (remote_ip);
CREATE INDEX idx_connections_protocol ON connections (protocol);
CREATE INDEX idx_connection_events_timestamp ON connection_events (timestamp);
CREATE INDEX idx_connection_events_connection_id ON connection_events (connection_id);
CREATE INDEX idx_domains_domain ON domains (domain);
CREATE INDEX idx_ip_domains_domain_id ON ip_domains (domain_id);
CREATE INDEX idx_ip_networks_network_id ON ip_networks (network_id);
