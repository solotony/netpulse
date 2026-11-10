# macOS collector (MVP)

The macOS collector is a **1-second `libproc` polling** fallback. It is **not** a Network Extension / System Extension in this MVP.

## Architecture

1. **Snapshot enumeration** — walk processes and socket FDs via `PROC_PIDLISTFDS` / `PROC_PIDFDSOCKETINFO` (live path) or inject fixture snapshots (tests).
2. **Diff engine** — compare consecutive snapshots and emit normalized `NewConnection` / `ConnectionClosed` events.
3. **Process enrichment** — resolve PID metadata through `process-info` (`ProcessSource`). Missing fields stay `null`; nothing is invented.

Unit tests exercise the diff engine with fixture snapshots and **do not** call real libproc.

## Timing and lifecycle

| Parameter | Value |
|---|---|
| Poll interval | **1 second** (`POLL_INTERVAL_MS = 1000`) |
| UDP idle close | **30 seconds** without a sighting (`UDP_IDLE_CLOSE_SECS = 30`) |

### TCP

- Present in snapshot after absence → `NewConnection` (includes sockets already open when monitoring starts).
- Absent from snapshot after presence → `ConnectionClosed` immediately.

### UDP

UDP has no reliable teardown signal under polling. A 5-tuple flow (plus owning PID) stays open while it is **reobserved**. If it is not seen for **30 seconds**, the collector emits `ConnectionClosed`.

### ICMP (`ping`)

macOS `ping` uses an unconnected ICMP datagram socket, so libproc reports `*.*` and no foreign address. When that socket belongs to `ping` or `ping6`, the collector reads the process arguments, resolves the destination, and records an outbound ICMP flow. The typed hostname is stored as an observed domain. Other unconnected ICMP sockets stay skipped.

### Short connections

Connections that open and close **between poll ticks may be missed**. A one-shot `ping -c 1` can exit between polls. Monitoring is best-effort for this polling path, not complete event-driven capture.

## Capabilities

Reported via `CollectorCapabilities` / `macos_capabilities()`:

| Capability | Value |
|---|---|
| New connection detection | true |
| Existing connection enumeration | true |
| Connection termination detection | true |
| TCP | true |
| UDP | true |
| Traffic byte counters | **false** (unavailable via this mechanism) |
| Process identification | true |
| Incoming connection detection | true |

Byte counters on emitted events are always `None`.

## Limitations

- Requires sufficient privileges to inspect other processes’ FDs; inaccessible processes leave name / path / start time unset.
- Listening sockets, and unconnected sockets whose peer cannot be recovered, are skipped. `ping` / `ping6` destinations are taken from the process arguments.
- No Network Extension entitlements, code signing, or system-extension install steps in the MVP path.
