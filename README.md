# Network Monitor

Standalone desktop application that continuously monitors network connections opened by local processes, enriches remote endpoints, and stores a searchable history in SQLite.

## Stack

- **Desktop:** Tauri 2
- **Backend:** Rust (Cargo workspace)
- **Frontend:** Vue 3 + TypeScript
- **Database:** SQLite via SQLx

## Run the app

```bash
cd apps/desktop
npm install
npm run tauri dev
```

Backend / workspace checks:

```bash
cargo test --workspace
```

## Known MVP limitations

- **1s poll:** The macOS collector enumerates sockets about once per second. Connections that open and close between poll ticks **may be missed**.
- **UDP idle close:** UDP flows with no observed activity are treated as closed after **30 seconds** of idle time.
- **Linux / Windows:** Collectors are stubs only — they report all capabilities as `false` and `start` returns not-implemented. They do **not** collect network activity yet.

## Docs

- [docs/assumptions.md](docs/assumptions.md) — MVP design decisions and known limitations
- [docs/macos-collector.md](docs/macos-collector.md) — macOS polling collector architecture and timing

## Workspace layout

```
.
├── apps/desktop/         # Tauri 2 + Vue 3 UI
├── crates/
│   ├── common/           # Shared types and utilities
│   ├── core/             # Lifecycle, collector management, event coordination
│   ├── collector-api/    # Common collector interface and capabilities
│   ├── collector-macos/  # macOS collector (libproc polling MVP)
│   ├── collector-linux/  # Linux collector stub (not collecting yet)
│   ├── collector-windows/# Windows collector stub (not collecting yet)
│   ├── process-info/     # PID → process identity
│   ├── dns-resolver/     # Reverse DNS (PTR)
│   ├── ip-enrichment/    # RDAP / network enrichment
│   ├── database/         # SQLite persistence, history, export
│   └── statistics/       # Historical aggregations
├── resources/            # Migrations and optional local data
├── docs/                 # Architecture notes and assumptions
├── Cargo.toml
└── task.md               # Product and technical specification
```

## Platform status

| Platform | Status |
|---|---|
| macOS | Fully implemented MVP target (1s libproc polling) |
| Linux | Stub only — does not collect yet |
| Windows | Stub only — does not collect yet |
