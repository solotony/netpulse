# Network Monitor — Product and Technical Specification

## 1. Project Overview

Develop a standalone desktop application called **Network Monitor**.

The application continuously monitors network connections opened by processes running on the local computer.

For every detected connection, the application should identify and record:

- The originating process.
- The process executable path.
- The process ID (PID).
- The local IP address and port.
- The remote IP address and port.
- The network protocol.
- The remote IP's domain name, when available.
- The IP address network range (CIDR).
- The network name (NetName).
- The organization owning the network.
- The ASN (Autonomous System Number).
- The country, when available.
- Connection start and end times.

All information must be stored locally and remain available after application restarts.

The application must support historical searches, filtering, statistics, and exporting data to plain text files.

### Supported operating systems

Priority:

1. macOS — mandatory, first fully implemented platform.
2. Linux — planned, architecture must support it.
3. Windows — planned, architecture must support it.

This is ONE application, not three separate products.

All platforms must use:
- The same application architecture.
- The same database schema.
- The same event model.
- The same business logic.
- The same UI structure.
- The same export formats.

Only platform-specific network collectors and OS integrations should differ.

---

# 2. Technology Stack

Use the following technologies:

| Component | Technology |
|---|---|
| Desktop framework | Tauri 2 |
| Backend | Rust |
| Frontend | Vue 3 |
| Frontend language | TypeScript |
| UI | Native desktop-style UI implemented through Tauri |
| Database | SQLite |
| Database access | SQLx |
| Async runtime | Tokio |
| Serialization | Serde |
| Logging | tracing |
| Frontend state | Pinia |
| Build system | Cargo + Tauri CLI |

Do not use Electron.

Do not implement a web-based server that must be started manually.

The application must be installed and launched as a normal desktop application.

It must not require a remote server, cloud account, or external database.

---

# 3. Architecture

Use a modular architecture with clearly separated responsibilities.

Recommended project structure:

    network-monitor/
    ├── apps/
    │   └── desktop/
    │       ├── src/
    │       │   ├── components/
    │       │   ├── pages/
    │       │   ├── layouts/
    │       │   ├── stores/
    │       │   ├── services/
    │       │   └── types/
    │       └── src-tauri/
    │           ├── src/
    │           └── tauri.conf.json
    │
    ├── crates/
    │   ├── core/
    │   ├── collector-api/
    │   ├── collector-macos/
    │   ├── collector-linux/
    │   ├── collector-windows/
    │   ├── process-info/
    │   ├── dns-resolver/
    │   ├── ip-enrichment/
    │   ├── database/
    │   ├── statistics/
    │   └── common/
    │
    ├── resources/
    │   ├── geoip/
    │   ├── asn/
    │   └── migrations/
    │
    ├── docs/
    ├── Cargo.toml
    └── README.md

The exact directory structure may be adjusted if a better Rust/Tauri architecture is justified.

## 3.1. Core modules

### Core

Responsible for:
- Application lifecycle.
- Collector management.
- Event processing.
- Event normalization.
- Coordination between modules.

### Collector API

Defines a common interface for all platform collectors.

### Process Info

Responsible for:
- Resolving PID to process information.
- Process name.
- Executable path.
- Process start time.
- Process identity.
- Detecting terminated processes.

Process identity must not rely on PID alone.

PIDs can be reused by the operating system. Use PID together with process start time or another stable process-instance identifier.

### DNS Resolver

Responsible for:
- Resolving remote IP addresses to domain names.
- Reverse DNS lookups.
- Correlating observed DNS queries with network connections, where possible.
- Caching results.
- Recording the source and reliability of domain information.

### IP Enrichment

Responsible for:
- CIDR identification.
- NetRange.
- NetName.
- ASN.
- Organization.
- Country.
- IP registration information.

### Database

Responsible for:
- SQLite initialization.
- Schema migrations.
- Persistent storage.
- Historical queries.
- Database maintenance.

### Statistics

Responsible for aggregating historical information by:
- Process.
- IP address.
- Domain.
- Network.
- Organization.
- Date and time interval.

---

# 4. Network Collectors

Collectors must be independent platform-specific modules.

The rest of the application must not depend on the implementation details of a particular operating system.

Define a common collector interface.

For example:

    pub trait NetworkCollector: Send + Sync {
        fn platform(&self) -> Platform;

        fn capabilities(&self) -> CollectorCapabilities;

        async fn start(&mut self) -> Result<(), CollectorError>;

        async fn stop(&mut self) -> Result<(), CollectorError>;
    }

The interface may be adjusted to fit Rust async trait conventions.

All collectors must produce events in the same normalized format.

## 4.1. macOS Collector

This is the first implementation priority.

Investigate and implement the most reliable available macOS networking monitoring mechanism.

Preferred approach:
- Network Extension / System Extension where appropriate.
- Native macOS APIs for process identification.
- Additional socket enumeration for discovering connections already active when monitoring starts.

Consider the macOS requirements for:
- System Extension registration.
- Network Extension entitlements.
- User approval.
- Code signing and distribution.
- Background execution.
- Application lifecycle.

A System Extension may require a separate native Swift component. This is acceptable.

The native component must communicate with the Rust core through a documented interface.

Do not assume that ordinary Tauri permissions are sufficient for system-wide network monitoring.

If a fully event-driven implementation is not immediately possible, implement a clearly documented fallback using native socket enumeration and periodic polling.

The fallback must:
- Discover currently active connections.
- Identify associated processes where possible.
- Detect newly opened connections.
- Detect closed connections.
- Avoid generating duplicate records.
- Clearly document its polling interval and limitations.

Never claim that monitoring is complete if the selected collection mechanism can miss short-lived connections.

## 4.2. Linux Collector

Prepare an independent Linux implementation.

Preferred technologies:
- eBPF for network connection events.
- Netlink where appropriate.
- /proc and process metadata for process identification.

The Linux implementation should eventually support systemd-based background execution.

## 4.3. Windows Collector

Prepare an independent Windows implementation.

Preferred technologies:
- Event Tracing for Windows (ETW).
- Windows Filtering Platform (WFP), where appropriate.
- Native process APIs.

The Windows implementation should eventually support Windows Service operation.

## 4.4. Collector capabilities

Collectors must expose their actual capabilities.

For example:

- New connection detection.
- Existing connection enumeration.
- Connection termination detection.
- TCP support.
- UDP support.
- Traffic byte counters.
- Process identification.
- Incoming connection detection.

The UI must not advertise unsupported capabilities as available.

---

# 5. Connection Event Model

All collectors must produce a normalized event.

Suggested structure:

    struct ConnectionEvent {
        timestamp: DateTime<Utc>,
        event_type: ConnectionEventType,

        process_id: u32,
        process_name: Option<String>,
        process_path: Option<String>,
        process_start_time: Option<DateTime<Utc>>,

        protocol: Protocol,

        local_ip: IpAddr,
        local_port: u16,

        remote_ip: IpAddr,
        remote_port: u16,

        direction: Direction,

        bytes_sent: Option<u64>,
        bytes_received: Option<u64>,
    }

Event types:

- NewConnection
- ConnectionClosed
- ConnectionStateChanged
- TrafficUpdate

Protocol types:

- TCP
- UDP
- ICMP
- Other

Direction:

- Outbound
- Inbound
- Unknown

Not every collector will provide every field.

Optional information must remain nullable rather than being filled with fabricated values.

## 5.1. Connection lifecycle

The application must distinguish between:

1. A connection instance.
2. An individual event associated with that connection.
3. A unique remote IP address.
4. A process instance.

A connection should have a unique internal ID.

Multiple connections to the same remote IP must be recorded independently.

Repeated observations of the same active connection must not create duplicate connection records.

UDP does not have the same connection lifecycle as TCP. Its handling must be documented and adapted to the capabilities of the collector.

---

# 6. IP Address Enrichment

When a new remote IP address is first discovered, attempt to enrich it.

Do not perform external enrichment requests for every connection.

Enrichment should be asynchronous and cached.

## 6.1. Required information

Example:

    Remote IP:       172.67.10.20
    NetRange:        172.64.0.0 - 172.71.255.255
    CIDR:            172.64.0.0/13
    NetName:         CLOUDFLARENET
    Organization:    Cloudflare, Inc.
    ASN:             AS13335
    Country:         US

The values above are illustrative.

## 6.2. Data sources

Consider:
- RDAP.
- WHOIS-compatible registration data.
- Local IP prefix databases.
- ASN databases.
- GeoIP databases.

Prefer local databases for frequently needed information.

External lookups should be:
- Asynchronous.
- Cached.
- Rate limited.
- Retried when appropriate.

Never block connection event collection while waiting for an external lookup.

## 6.3. Special addresses

Handle correctly:
- Private IPv4 addresses.
- Loopback addresses.
- Link-local addresses.
- IPv6 unique local addresses.
- IPv6 link-local addresses.
- Multicast.
- Unspecified addresses.

Do not attempt public registration lookups for addresses that are not globally routable.

## 6.4. Network matching

Use longest-prefix matching when associating an IP with a known network.

Store the original network information and its update timestamp.

One IP address should be associated with the most specific applicable network prefix.

---

# 7. Domain Resolution

Domain resolution is an important feature, but an IP address does not necessarily have one unique domain.

For example, multiple websites can share the same Cloudflare IP address.

Use several possible sources:

1. Observed DNS queries and responses.
2. Operating system DNS cache, when accessible.
3. Reverse DNS (PTR).
4. Other reliable platform-specific metadata, where available.

Store:
- Domain name.
- Associated IP.
- Discovery source.
- Discovery timestamp.
- Last confirmation timestamp.
- Confidence or reliability information.

Suggested source types:

- DNS_OBSERVED
- DNS_CACHE
- REVERSE_DNS
- OTHER

Do not assume that reverse DNS returns the actual website visited.

Do not use TLS interception or install a certificate authority to inspect encrypted traffic.

If the actual domain cannot be determined, show it as unknown.

Multiple domains may be associated with the same IP.

---

# 8. Database Design

Use SQLite as the primary local database.

Enable:
- Foreign keys.
- WAL mode.
- Appropriate indexes.
- Transactional writes.
- Schema migrations.

The database must be able to store a long history of network activity.

Recommended tables:

## 8.1. processes

Stores process information.

Fields:

- id
- pid
- process_name
- executable_path
- process_start_time
- first_seen_at
- last_seen_at

Use a stable process-instance identifier.

## 8.2. connections

Stores connection sessions.

Fields:

- id
- process_id
- protocol
- direction
- local_ip
- local_port
- remote_ip
- remote_port
- started_at
- ended_at
- bytes_sent
- bytes_received
- collector_id

## 8.3. connection_events

Stores individual connection events when needed for historical reconstruction and diagnostics.

Fields:

- id
- connection_id
- event_type
- timestamp
- event_data

## 8.4. ip_addresses

Stores unique IP addresses.

Fields:

- id
- ip
- first_seen_at
- last_seen_at

## 8.5. domains

Stores unique domain names.

Fields:

- id
- domain
- first_seen_at
- last_seen_at

## 8.6. ip_domains

Many-to-many relationship between IP addresses and domains.

Fields:

- ip_id
- domain_id
- source
- first_seen_at
- last_seen_at
- confidence

## 8.7. networks

Stores network registration information.

Fields:

- id
- cidr
- net_range
- net_name
- organization
- asn
- country
- source
- updated_at

## 8.8. ip_networks

Associates IP addresses with networks.

Fields:

- ip_id
- network_id

The final schema may be improved during implementation.

Use appropriate indexes for:
- Timestamp.
- Process.
- Remote IP.
- Domain.
- Network.
- Protocol.

Avoid storing redundant copies of enrichment information in every connection record.

---

# 9. Application UI

The UI should be intentionally simple.

The primary goal is to display large amounts of information in tables.

Do not implement dashboards with charts.

Do not implement complicated visualizations.

Do not prioritize decorative elements over usability.

Use a clean native desktop application appearance.

The application should have a sidebar with these sections:

1. Connections
2. Processes
3. IP Addresses
4. Domains
5. Networks
6. Statistics
7. Settings

## 9.1. Connections

The main application screen.

Display a table with columns:

| Column | Description |
|---|---|
| Time | Connection start time |
| Process | Process name |
| PID | Process ID |
| Protocol | TCP/UDP/etc. |
| Direction | Incoming/outgoing |
| Local Address | Local IP and port |
| Remote Address | Remote IP and port |
| Domain | Resolved domain |
| Network | CIDR |
| Organization | Network owner |
| Duration | Connection duration |
| Status | Active/closed |

Features:
- Sorting by columns.
- Text search.
- Filtering by process.
- Filtering by IP.
- Filtering by domain.
- Filtering by network.
- Filtering by protocol.
- Filtering by date range.
- Active-only filter.
- Pagination or virtual scrolling.

Allow copying cell values.

Allow selecting and exporting multiple rows.

For a large history, do not load all records into frontend memory.

Use backend pagination.

## 9.2. Processes

Display a table of known processes.

Columns:

- Process name.
- PID.
- Executable path.
- First seen.
- Last seen.
- Total connections.
- Unique remote IP addresses.
- Unique domains.
- Total bytes sent.
- Total bytes received.

Selecting a process should display its associated connection history.

## 9.3. IP Addresses

Display a table of unique remote IP addresses.

Columns:

- IP address.
- First seen.
- Last seen.
- Number of connections.
- Associated processes.
- Associated domains.
- CIDR.
- NetName.
- ASN.
- Organization.
- Country.

Selecting an IP should display its connection history.

## 9.4. Domains

Display:

- Domain.
- Associated IP addresses.
- Associated processes.
- First seen.
- Last seen.
- Number of connections.
- Resolution source.

Multiple domains may point to the same IP.

## 9.5. Networks

Display:

- CIDR.
- NetRange.
- NetName.
- ASN.
- Organization.
- Country.
- Number of IP addresses.
- Number of connections.
- First seen.
- Last seen.

Selecting a network should display all associated IP addresses and connections.

## 9.6. Statistics

Statistics must be displayed as tables only.

Provide grouping by:

- Process.
- IP address.
- Domain.
- Network.
- Organization.
- Day.
- Week.
- Month.

Provide date range filtering.

Example:

| Process | Connections | Unique IPs | Domains |
|---|---:|---:|---:|
| Safari | 1240 | 87 | 65 |
| Chrome | 3410 | 152 | 130 |
| Telegram | 520 | 24 | 12 |

Statistics should be calculated from the stored database, not only from currently active connections.

---

# 10. Text Export

Export is an important feature.

All major tables must support exporting their data.

Supported formats:

1. Plain text (.txt).
2. Tab-separated text (.tsv).

Do not implement PDF, Excel, or complex report generation in the MVP.

## 10.1. Plain text export

Use readable, structured text.

Example:

    NETWORK MONITOR
    Export date: 2026-11-09
    Filter: Process = Safari
    Date range: 2026-11-01 - 2026-11-09

    ==================================================

    TIME:         2026-11-09 00:45:12
    PROCESS:      Safari
    PID:          1234
    PROTOCOL:     TCP
    DIRECTION:    OUTBOUND
    LOCAL:        192.168.1.10:54321
    REMOTE:       172.67.10.20:443
    DOMAIN:       example.com
    CIDR:         172.64.0.0/13
    NETNAME:      CLOUDFLARENET
    ASN:          AS13335
    ORGANIZATION: Cloudflare, Inc.
    STATUS:       CLOSED

    ==================================================

The actual export must include all selected records.

## 10.2. TSV export

Use tab-separated columns.

This format should be suitable for opening in spreadsheet applications or processing with scripts.

## 10.3. Export options

Allow exporting:
- All records.
- Currently filtered records.
- Selected records.
- A specific date range.
- A process history.
- An IP history.
- A network history.

Export must run in the backend to avoid loading large datasets into the frontend.

Use streaming or chunked file generation for large exports.

---

# 11. Background Operation

The application must continue collecting network information when its main window is closed or minimized.

The user should not need to keep the main window open.

Implement a background component with an appropriate lifecycle for each operating system.

Possible mechanisms:

| OS | Mechanism |
|---|---|
| macOS | LaunchAgent and required System Extension |
| Linux | systemd service |
| Windows | Windows Service |

The desktop application and background collector should share the same core logic.

Use local IPC between the UI and a separate background process if required by platform architecture.

The installer must install and configure the required components.

The user must be able to:
- Start monitoring.
- Stop monitoring.
- Pause monitoring.
- Resume monitoring.
- Open the main window.
- See collector status.

Show a clear indication when monitoring is not working.

Do not silently lose events when the collector fails.

Record collector errors in the application log.

---

# 12. Settings

Implement a simple settings page.

Settings should include:

### Monitoring

- Start monitoring automatically.
- Start application at login.
- Run in background after closing the window.
- Enable/disable monitoring.
- Collector status.

### Data

- Database location.
- Database size.
- First recorded event.
- Last recorded event.
- Export database.
- Clear history.
- Optional automatic history cleanup.

### Enrichment

- Enable DNS resolution.
- Enable reverse DNS.
- Enable IP registration lookups.
- Enable ASN information.
- Enable country information.
- Cache expiration settings.

### Interface

- Application language.
- Theme: system/light/dark.
- Date and time format.

---

# 13. Performance Requirements

The application must be suitable for continuous operation.

Requirements:

- Low CPU usage while idle.
- Efficient event processing.
- Asynchronous DNS and IP enrichment.
- No blocking network collection while resolving domains.
- Batched database writes.
- Proper SQLite indexing.
- No unbounded memory growth.
- No unnecessary repeated DNS or RDAP queries.
- Efficient historical queries.
- Support for hundreds of thousands or millions of historical connection records.

The UI must remain responsive while collection continues.

Use pagination or virtual scrolling for large tables.

---

# 14. Security and Privacy

This is a local monitoring application.

Requirements:

- All historical data is stored locally.
- No telemetry.
- No analytics sent to third parties.
- No mandatory cloud services.
- No user account.
- No remote database.
- No automatic transmission of connection history.

External requests may only be made for DNS, RDAP, and IP enrichment when enabled.

The user must be able to disable external enrichment.

The application must never modify network traffic.

Do not implement firewall rules or connection blocking in the MVP.

Do not intercept encrypted application traffic.

Do not collect payload contents.

---

# 15. Testing

Implement automated tests.

Required test categories:

### Unit tests

- IP address normalization.
- IPv4 and IPv6 handling.
- CIDR matching.
- Longest-prefix network matching.
- Domain normalization.
- Domain-to-IP relationships.
- Process identity handling.
- Event deduplication.
- Connection lifecycle.
- Statistics aggregation.
- Export formatting.

### Database tests

- Schema migrations.
- Insert and update operations.
- Historical queries.
- Pagination.
- Filtering.
- Aggregation.
- Large exports.

### Collector tests

Use mock collectors to test the common collector API.

Test:
- Starting and stopping.
- Event delivery.
- Collector failure.
- Duplicate event handling.
- Missing optional fields.
- Reconnection and recovery.

Do not pretend that a mock collector is a real OS collector.

### Integration tests

Test the complete pipeline:

Collector → Event Processing → Enrichment → SQLite → UI → Export.

---

# 16. Development Strategy

Implement the project incrementally.

Do not attempt to implement all operating systems simultaneously.

## Phase 1 — Project Foundation

- Initialize Tauri 2.
- Configure Vue 3 and TypeScript.
- Create the Rust Cargo workspace.
- Implement module boundaries.
- Implement SQLite.
- Create database migrations.
- Implement the collector API.
- Create a mock collector.
- Implement the initial UI.

Deliverable: application launches and displays mock network connection data.

## Phase 2 — macOS Collector

- Investigate the available macOS collection APIs.
- Implement discovery of existing connections.
- Implement new connection detection.
- Implement process identification.
- Implement connection lifecycle tracking.
- Implement background operation.
- Integrate the collector with the core.

Deliverable: real macOS network activity is recorded and displayed.

## Phase 3 — Enrichment

- DNS resolution.
- Reverse DNS.
- IP-to-CIDR matching.
- NetName.
- ASN.
- Organization.
- Country.
- Caching and asynchronous processing.

Deliverable: newly discovered IP addresses are enriched automatically.

## Phase 4 — History and Statistics

- Historical connection storage.
- Process statistics.
- IP statistics.
- Domain statistics.
- Network statistics.
- Date-based aggregation.
- Search and filtering.

Deliverable: complete historical analysis using tables.

## Phase 5 — Export

- Plain text export.
- TSV export.
- Filtered export.
- Selected records export.
- Large export handling.

Deliverable: users can export any relevant historical data.

## Phase 6 — Linux and Windows

Implement independent platform collectors.

Do not change the common event model or database schema unless a documented requirement justifies it.

Deliverable: Linux and Windows builds using the same application and UI.

---

# 17. Implementation Rules for Cursor

Before implementing anything:

1. Inspect the existing repository.
2. Check whether there is already code, configuration, documentation, or an established architecture.
3. Do not overwrite existing work without understanding it.
4. Prepare an implementation plan.
5. Identify platform-specific technical limitations.
6. Document any assumptions.

Development rules:

- Prefer small, testable modules.
- Keep platform-specific code isolated.
- Do not put database logic inside UI components.
- Do not put OS-specific collector logic inside the common core.
- Use explicit error handling.
- Use structured logging.
- Use database migrations.
- Keep Rust types strongly typed.
- Avoid unnecessary dependencies.
- Do not introduce a server/cloud architecture.
- Do not create fake functionality to make the UI look complete.
- Do not report unsupported platform features as implemented.
- Add automated tests alongside implementation.
- Update documentation as the architecture evolves.

If a feature requires special macOS entitlements, system extensions, administrator privileges, or OS-specific installation steps, document them explicitly.

If an OS API cannot provide a particular piece of information, represent it as unavailable rather than inventing data.

---

# 18. Definition of Done

The initial MVP is complete when:

1. The application builds and launches on macOS.
2. The application can monitor real network connections.
3. Existing connections can be discovered when monitoring starts.
4. New connections are detected.
5. Process names and PIDs are identified where permissions and APIs allow.
6. Remote IP addresses are recorded.
7. TCP and UDP support is implemented according to documented collector capabilities.
8. IP enrichment works.
9. Domain resolution works where information is available.
10. All collected information is persisted in SQLite.
11. Historical data survives application restarts.
12. The UI displays connections in a searchable, sortable table.
13. Process, IP, domain, and network tables work.
14. Historical statistics are available.
15. Data can be exported to TXT and TSV.
16. Monitoring continues in the background.
17. Automated tests cover the core functionality.
18. The application does not require any external server.

The first release should prioritize reliable data collection, correct historical storage, and useful tables over visual complexity.

**The main purpose of Network Monitor is to build a persistent, searchable history of which processes communicate with which remote IP addresses, domains, and networks, and when those communications occur.**