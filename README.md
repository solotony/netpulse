# Network Monitor

Standalone desktop application that continuously monitors network connections opened by local processes, enriches remote endpoints, and stores a searchable history in SQLite.

## Stack

- **Desktop:** Tauri 2
- **Backend:** Rust (Cargo workspace)
- **Frontend:** Vue 3 + TypeScript
- **Database:** SQLite via SQLx

## Workspace layout

```
.
├── apps/                 # Desktop UI (Tauri + Vue) — added in later steps
├── crates/
│   ├── common/           # Shared types and utilities
│   ├── core/             # Lifecycle, collector management, event coordination
│   ├── collector-api/    # Common collector interface and capabilities
│   ├── collector-macos/  # macOS collector (libproc polling MVP)
│   ├── collector-linux/  # Linux collector stub
│   ├── collector-windows/# Windows collector stub
│   ├── process-info/     # PID → process identity
│   ├── dns-resolver/     # Domain / reverse DNS
│   ├── ip-enrichment/    # RDAP / network enrichment
│   ├── database/         # SQLite persistence
│   └── statistics/       # Historical aggregations
├── resources/            # Migrations and optional local data
├── docs/                 # Architecture notes and assumptions
├── Cargo.toml
└── task.md               # Product and technical specification
```

## Building

```bash
cargo test --workspace
```

## Platform status

| Platform | Status |
|---|---|
| macOS | First fully implemented target (1s libproc polling MVP) |
| Linux | Crate stub; `start` returns not-implemented |
| Windows | Crate stub; `start` returns not-implemented |

See [docs/assumptions.md](docs/assumptions.md) for MVP design decisions and known limitations.
