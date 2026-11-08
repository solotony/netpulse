# Network Monitor — Desktop

Tauri 2 + Vue 3 + TypeScript + Pinia shell. Connections load from a mock collector into SQLite.

## Develop

```bash
cd apps/desktop
npm install
npm run tauri dev
```

## Test

```bash
# Frontend (Vitest)
npm test

# Backend (from repo root)
cargo test -p network-monitor
cargo test --workspace
```
