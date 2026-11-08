# Network Monitor — MVP Assumptions

Documented decisions for Phase 1 / Step 1. These constrain the first working release and must stay visible in the UI and docs where capabilities differ from the ideal architecture in `task.md`.

## macOS collector

- **Mechanism:** 1-second `libproc` polling for socket enumeration — **not** a Network Extension / System Extension in the MVP.
- **UDP lifecycle:** UDP flows with no observed activity are treated as closed after **30 seconds** of idle time.
- **Short connections:** Connections that open and close between poll ticks may be **missed**. Monitoring is best-effort for the polling fallback, not complete event-driven capture.
- **Byte counters:** Traffic byte counters are **unavailable** via this mechanism (`traffic_byte_counters: false`).

## Application process model

- **Single process:** UI and collector run in **one process** (Tauri host + Rust backend).
- **Window close:** Closing the main window **hides to the system tray** when background mode is enabled (default); monitoring continues. Quit from the tray menu (or with background mode off) stops the collector.
- **Login autostart:** Use **Tauri autostart** so the app can launch at user login (persisted in SQLite settings).
- **Tray icon:** Uses the default app window icon (not a dedicated monochrome menu-bar template).

## Enrichment and DNS

- **Non-blocking:** Enrichment **never blocks** connection event collection. Lookups run asynchronously and are cached / rate-limited.
- **Special-use IPs:** Private, loopback, link-local, multicast, and other non-globally-routable addresses **skip RDAP** / public registration lookups.
- **GeoIP / ASN databases:** **No MaxMind bundle** ships with the MVP. Country / ASN may come from registration data when available, or remain unknown.
- **DNS MVP:** Domain resolution for the MVP is **reverse DNS (PTR) only**. Observed DNS / OS cache correlation is deferred.

## Linux and Windows stubs

- `collector-linux` and `collector-windows` crates exist for workspace structure and future work.
- `start()` returns **not-implemented**.
- Reported **capabilities are all `false`** so the UI must not advertise unsupported features.
