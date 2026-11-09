//! Streaming TXT / TSV export for connection history.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, QueryBuilder, SqlitePool};

use crate::error::DatabaseError;
use crate::queries::{
    push_filters, ASN_SUBQUERY, DOMAIN_SUBQUERY, NETWORK_CIDR_SUBQUERY, NETWORK_NAME_SUBQUERY,
    ORGANIZATION_SUBQUERY, ConnectionHistoryFilter, MAX_PAGE_SIZE,
};

/// Default page size used while streaming export rows from SQLite.
pub const EXPORT_CHUNK_SIZE: u32 = 250;

/// TSV column header names (stable spreadsheet layout).
pub const TSV_COLUMNS: &[&str] = &[
    "time",
    "process",
    "pid",
    "protocol",
    "direction",
    "local",
    "remote",
    "domain",
    "cidr",
    "netname",
    "asn",
    "organization",
    "status",
];

/// Output file format for connection-history export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Txt,
    Tsv,
}

/// Options controlling which rows are exported and how they are formatted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    pub filter: ConnectionHistoryFilter,
    /// When `Some`, only these connection ids are exported (may still AND with filter).
    pub selected_ids: Option<Vec<i64>>,
    pub format: ExportFormat,
    /// Stamp written into the TXT header (`Export date: …`).
    pub export_date: DateTime<Utc>,
    /// Rows fetched per SQL page while streaming.
    pub chunk_size: u32,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            filter: ConnectionHistoryFilter::default(),
            selected_ids: None,
            format: ExportFormat::Txt,
            export_date: Utc::now(),
            chunk_size: EXPORT_CHUNK_SIZE,
        }
    }
}

/// Result of a completed export stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportSummary {
    pub rows_written: u64,
}

#[derive(Debug, Clone, FromRow)]
struct ExportRow {
    #[allow(dead_code)]
    id: i64,
    process_name: Option<String>,
    pid: i64,
    protocol: String,
    direction: String,
    local_ip: String,
    local_port: i64,
    remote_ip: String,
    remote_port: i64,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    domain: Option<String>,
    network_cidr: Option<String>,
    network_name: Option<String>,
    asn: Option<i64>,
    organization: Option<String>,
}

/// Builds a human-readable filter summary for the TXT header.
pub fn describe_export_filter(
    filter: &ConnectionHistoryFilter,
    selected_ids: Option<&[i64]>,
) -> String {
    let mut parts = Vec::new();
    if let Some(ids) = selected_ids {
        parts.push(format!("Selected = {} row(s)", ids.len()));
    }
    if let Some(ref text) = filter.text {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            parts.push(format!("Text = {trimmed}"));
        }
    }
    if let Some(process_id) = filter.process_id {
        parts.push(format!("ProcessId = {process_id}"));
    }
    if let Some(ref name) = filter.process_name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            parts.push(format!("Process = {trimmed}"));
        }
    }
    if let Some(ref ip) = filter.remote_ip {
        let trimmed = ip.trim();
        if !trimmed.is_empty() {
            parts.push(format!("IP = {trimmed}"));
        }
    }
    if let Some(ref domain) = filter.domain {
        let trimmed = domain.trim();
        if !trimmed.is_empty() {
            parts.push(format!("Domain = {trimmed}"));
        }
    }
    if let Some(ref network) = filter.network {
        let trimmed = network.trim();
        if !trimmed.is_empty() {
            parts.push(format!("Network = {trimmed}"));
        }
    }
    if let Some(ref protocol) = filter.protocol {
        let trimmed = protocol.trim();
        if !trimmed.is_empty() {
            parts.push(format!("Protocol = {}", trimmed.to_ascii_uppercase()));
        }
    }
    if filter.active_only {
        parts.push("Active only".to_owned());
    }
    if parts.is_empty() {
        "None".to_owned()
    } else {
        parts.join("; ")
    }
}

fn format_date_range(filter: &ConnectionHistoryFilter) -> Option<String> {
    match (filter.started_from, filter.started_to) {
        (None, None) => None,
        (Some(from), None) => Some(format!("{} - …", format_date(from))),
        (None, Some(to)) => Some(format!("… - {}", format_date(to))),
        (Some(from), Some(to)) => Some(format!("{} - {}", format_date(from), format_date(to))),
    }
}

fn format_date(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d").to_string()
}

fn format_timestamp(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn display_opt(value: Option<&str>) -> &str {
    value.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("")
}

fn endpoint(ip: &str, port: i64) -> String {
    format!("{ip}:{port}")
}

fn status_label(ended_at: Option<DateTime<Utc>>) -> &'static str {
    if ended_at.is_some() {
        "CLOSED"
    } else {
        "ACTIVE"
    }
}

fn asn_label(asn: Option<i64>) -> String {
    match asn {
        Some(v) => format!("AS{v}"),
        None => String::new(),
    }
}

fn write_txt_field(out: &mut String, label: &str, value: &str) {
    let head = format!("{label}:");
    if head.len() < 14 {
        out.push_str(&head);
        for _ in 0..(14 - head.len()) {
            out.push(' ');
        }
        out.push_str(value);
        out.push('\n');
    } else {
        out.push_str(&head);
        out.push(' ');
        out.push_str(value);
        out.push('\n');
    }
}

fn write_txt_header(
    writer: &mut dyn Write,
    options: &ExportOptions,
) -> Result<(), DatabaseError> {
    let mut buf = String::with_capacity(256);
    buf.push_str("NETWORK MONITOR\n");
    buf.push_str(&format!(
        "Export date: {}\n",
        format_date(options.export_date)
    ));
    buf.push_str(&format!(
        "Filter: {}\n",
        describe_export_filter(&options.filter, options.selected_ids.as_deref())
    ));
    if let Some(range) = format_date_range(&options.filter) {
        buf.push_str(&format!("Date range: {range}\n"));
    }
    buf.push('\n');
    writer.write_all(buf.as_bytes())?;
    Ok(())
}

fn write_txt_row(writer: &mut dyn Write, row: &ExportRow) -> Result<(), DatabaseError> {
    // Bound per-row buffer — never accumulate the whole export in one String.
    let mut buf = String::with_capacity(512);
    buf.push_str("==================================================\n\n");
    write_txt_field(&mut buf, "TIME", &format_timestamp(row.started_at));
    write_txt_field(&mut buf, "PROCESS", display_opt(row.process_name.as_deref()));
    write_txt_field(&mut buf, "PID", &row.pid.to_string());
    write_txt_field(&mut buf, "PROTOCOL", &row.protocol.to_ascii_uppercase());
    write_txt_field(&mut buf, "DIRECTION", &row.direction.to_ascii_uppercase());
    write_txt_field(
        &mut buf,
        "LOCAL",
        &endpoint(&row.local_ip, row.local_port),
    );
    write_txt_field(
        &mut buf,
        "REMOTE",
        &endpoint(&row.remote_ip, row.remote_port),
    );
    write_txt_field(&mut buf, "DOMAIN", display_opt(row.domain.as_deref()));
    write_txt_field(&mut buf, "CIDR", display_opt(row.network_cidr.as_deref()));
    write_txt_field(&mut buf, "NETNAME", display_opt(row.network_name.as_deref()));
    write_txt_field(&mut buf, "ASN", &asn_label(row.asn));
    write_txt_field(
        &mut buf,
        "ORGANIZATION",
        display_opt(row.organization.as_deref()),
    );
    write_txt_field(&mut buf, "STATUS", status_label(row.ended_at));
    buf.push('\n');
    writer.write_all(buf.as_bytes())?;
    Ok(())
}

/// Escapes a TSV field (RFC-style quoting when tabs/newlines/quotes are present).
pub fn escape_tsv_field(value: &str) -> String {
    if value.contains('\t') || value.contains('\n') || value.contains('\r') || value.contains('"')
    {
        let mut escaped = String::with_capacity(value.len() + 2);
        escaped.push('"');
        for ch in value.chars() {
            if ch == '"' {
                escaped.push('"');
            }
            escaped.push(ch);
        }
        escaped.push('"');
        escaped
    } else {
        value.to_owned()
    }
}

fn write_tsv_header(writer: &mut dyn Write) -> Result<(), DatabaseError> {
    let line = TSV_COLUMNS.join("\t");
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn write_tsv_row(writer: &mut dyn Write, row: &ExportRow) -> Result<(), DatabaseError> {
    let fields = [
        format_timestamp(row.started_at),
        display_opt(row.process_name.as_deref()).to_owned(),
        row.pid.to_string(),
        row.protocol.to_ascii_uppercase(),
        row.direction.to_ascii_uppercase(),
        endpoint(&row.local_ip, row.local_port),
        endpoint(&row.remote_ip, row.remote_port),
        display_opt(row.domain.as_deref()).to_owned(),
        display_opt(row.network_cidr.as_deref()).to_owned(),
        display_opt(row.network_name.as_deref()).to_owned(),
        asn_label(row.asn),
        display_opt(row.organization.as_deref()).to_owned(),
        status_label(row.ended_at).to_owned(),
    ];
    let mut buf = String::with_capacity(256);
    for (idx, field) in fields.iter().enumerate() {
        if idx > 0 {
            buf.push('\t');
        }
        buf.push_str(&escape_tsv_field(field));
    }
    buf.push('\n');
    writer.write_all(buf.as_bytes())?;
    Ok(())
}

async fn fetch_export_page(
    pool: &SqlitePool,
    filter: &ConnectionHistoryFilter,
    ids: Option<&[i64]>,
    limit: u32,
    offset: u64,
) -> Result<Vec<ExportRow>, DatabaseError> {
    let mut qb = QueryBuilder::new(
        r#"
        SELECT
            c.id AS id,
            p.process_name AS process_name,
            p.pid AS pid,
            c.protocol AS protocol,
            c.direction AS direction,
            c.local_ip AS local_ip,
            c.local_port AS local_port,
            c.remote_ip AS remote_ip,
            c.remote_port AS remote_port,
            c.started_at AS started_at,
            c.ended_at AS ended_at,
        "#,
    );
    qb.push(DOMAIN_SUBQUERY);
    qb.push(" AS domain, ");
    qb.push(NETWORK_CIDR_SUBQUERY);
    qb.push(" AS network_cidr, ");
    qb.push(NETWORK_NAME_SUBQUERY);
    qb.push(" AS network_name, ");
    qb.push(ASN_SUBQUERY);
    qb.push(" AS asn, ");
    qb.push(ORGANIZATION_SUBQUERY);
    qb.push(
        r#"
         AS organization
        FROM connections c
        INNER JOIN processes p ON p.id = c.process_id
        WHERE 1 = 1
        "#,
    );
    push_filters(&mut qb, filter, ids);
    qb.push(" ORDER BY c.started_at DESC, c.id DESC");
    qb.push(" LIMIT ");
    qb.push_bind(i64::from(limit));
    qb.push(" OFFSET ");
    qb.push_bind(offset as i64);

    let rows = qb.build_query_as::<ExportRow>().fetch_all(pool).await?;
    Ok(rows)
}

/// Streams connection history into `writer` without building one giant in-memory String.
pub async fn export_connection_history<W: Write>(
    pool: &SqlitePool,
    options: &ExportOptions,
    writer: &mut W,
) -> Result<ExportSummary, DatabaseError> {
    let chunk = options.chunk_size.clamp(1, MAX_PAGE_SIZE);
    let ids = options.selected_ids.as_deref();

    match options.format {
        ExportFormat::Txt => write_txt_header(writer, options)?,
        ExportFormat::Tsv => write_tsv_header(writer)?,
    }

    let mut offset: u64 = 0;
    let mut rows_written: u64 = 0;

    loop {
        let page = fetch_export_page(pool, &options.filter, ids, chunk, offset).await?;
        if page.is_empty() {
            break;
        }
        let page_len = page.len() as u64;
        for row in &page {
            match options.format {
                ExportFormat::Txt => write_txt_row(writer, row)?,
                ExportFormat::Tsv => write_tsv_row(writer, row)?,
            }
        }
        // Drop the page before fetching the next chunk so peak memory stays bounded.
        drop(page);
        writer.flush()?;
        rows_written += page_len;
        offset += page_len;
        if page_len < u64::from(chunk) {
            break;
        }
    }

    if options.format == ExportFormat::Txt && rows_written > 0 {
        writer.write_all(b"==================================================\n")?;
        writer.flush()?;
    }

    Ok(ExportSummary { rows_written })
}

/// Opens `path` and streams an export to it.
pub async fn export_connection_history_to_path(
    pool: &SqlitePool,
    options: &ExportOptions,
    path: impl AsRef<Path>,
) -> Result<ExportSummary, DatabaseError> {
    let file = File::create(path.as_ref())?;
    let mut writer = BufWriter::with_capacity(64 * 1024, file);
    let summary = export_connection_history(pool, options, &mut writer).await?;
    writer.flush()?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::cell::Cell;
    use std::rc::Rc;

    use crate::models::{NewConnection, NewIpDomain, NewNetwork, NewProcess};
    use crate::writes::{
        insert_connection, insert_process, store_winning_ip_network, update_connection,
        upsert_ip_domain,
    };
    use crate::{ConnectionUpdate, Database};

    fn t(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, s).unwrap()
    }

    /// Writer that records every write length and rejects oversized single writes.
    struct ChunkProbeWriter {
        max_write: usize,
        writes: Rc<Cell<usize>>,
        max_seen: Rc<Cell<usize>>,
        bytes: Rc<Cell<usize>>,
        data: Vec<u8>,
    }

    impl ChunkProbeWriter {
        fn new(max_write: usize) -> Self {
            Self {
                max_write,
                writes: Rc::new(Cell::new(0)),
                max_seen: Rc::new(Cell::new(0)),
                bytes: Rc::new(Cell::new(0)),
                data: Vec::new(),
            }
        }
    }

    impl Write for ChunkProbeWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            assert!(
                buf.len() <= self.max_write,
                "single write of {} bytes exceeds max {}",
                buf.len(),
                self.max_write
            );
            self.writes.set(self.writes.get() + 1);
            self.max_seen
                .set(self.max_seen.get().max(buf.len()));
            self.bytes.set(self.bytes.get() + buf.len());
            self.data.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    async fn seed_enriched(db: &Database) -> i64 {
        let process_id = insert_process(
            db.pool(),
            &NewProcess {
                pid: 1234,
                process_name: Some("Safari".into()),
                executable_path: None,
                process_start_time: t(2026, 11, 1, 0, 0, 0),
                first_seen_at: t(2026, 11, 1, 0, 0, 0),
                last_seen_at: t(2026, 11, 9, 0, 0, 0),
            },
        )
        .await
        .expect("process");

        let conn_id = insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "192.168.1.10".into(),
                local_port: 54321,
                remote_ip: "172.67.10.20".into(),
                remote_port: 443,
                started_at: t(2026, 11, 9, 0, 45, 12),
                ended_at: None,
                bytes_sent: Some(1),
                bytes_received: Some(2),
                collector_id: Some("test".into()),
            },
        )
        .await
        .expect("connection");

        update_connection(
            db.pool(),
            conn_id,
            &ConnectionUpdate {
                ended_at: Some(Some(t(2026, 11, 9, 0, 46, 0))),
                bytes_sent: None,
                bytes_received: None,
                collector_id: None,
            },
        )
        .await
        .expect("close");

        upsert_ip_domain(
            db.pool(),
            &NewIpDomain {
                ip: "172.67.10.20".into(),
                domain: "example.com".into(),
                source: "REVERSE_DNS".into(),
                confidence: Some(0.8),
                seen_at: t(2026, 11, 9, 0, 45, 12),
            },
        )
        .await
        .expect("domain");

        store_winning_ip_network(
            db.pool(),
            "172.67.10.20",
            &NewNetwork {
                cidr: "172.64.0.0/13".into(),
                net_range: None,
                net_name: Some("CLOUDFLARENET".into()),
                organization: Some("Cloudflare, Inc.".into()),
                asn: Some(13335),
                country: Some("US".into()),
                source: Some("rdap".into()),
                updated_at: t(2026, 11, 9, 0, 45, 12),
            },
            t(2026, 11, 9, 0, 45, 12),
        )
        .await
        .expect("network");

        conn_id
    }

    #[test]
    fn tsv_escaping_quotes_tabs_and_newlines() {
        assert_eq!(escape_tsv_field("plain"), "plain");
        assert_eq!(escape_tsv_field("a\tb"), "\"a\tb\"");
        assert_eq!(escape_tsv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(escape_tsv_field("line\nbreak"), "\"line\nbreak\"");
    }

    #[tokio::test]
    async fn txt_block_contains_every_selected_field() {
        let db = Database::open_in_memory().await.expect("open");
        let _id = seed_enriched(&db).await;

        let mut out = Vec::new();
        let summary = export_connection_history(
            db.pool(),
            &ExportOptions {
                filter: ConnectionHistoryFilter {
                    process_name: Some("Safari".into()),
                    started_from: Some(t(2026, 11, 1, 0, 0, 0)),
                    started_to: Some(t(2026, 11, 9, 23, 59, 59)),
                    ..ConnectionHistoryFilter::default()
                },
                selected_ids: None,
                format: ExportFormat::Txt,
                export_date: t(2026, 11, 9, 12, 0, 0),
                chunk_size: 50,
            },
            &mut out,
        )
        .await
        .expect("export");
        assert_eq!(summary.rows_written, 1);

        let text = String::from_utf8(out).expect("utf8");
        assert!(text.starts_with("NETWORK MONITOR\n"));
        assert!(text.contains("Export date: 2026-11-09\n"));
        assert!(text.contains("Filter: Process = Safari\n"));
        assert!(text.contains("Date range: 2026-11-01 - 2026-11-09\n"));
        for field in [
            "TIME:",
            "PROCESS:",
            "PID:",
            "PROTOCOL:",
            "DIRECTION:",
            "LOCAL:",
            "REMOTE:",
            "DOMAIN:",
            "CIDR:",
            "NETNAME:",
            "ASN:",
            "ORGANIZATION:",
            "STATUS:",
        ] {
            assert!(text.contains(field), "missing field label {field}");
        }
        assert!(text.contains("TIME:         2026-11-09 00:45:12"));
        assert!(text.contains("PROCESS:      Safari"));
        assert!(text.contains("PID:          1234"));
        assert!(text.contains("PROTOCOL:     TCP"));
        assert!(text.contains("DIRECTION:    OUTBOUND"));
        assert!(text.contains("LOCAL:        192.168.1.10:54321"));
        assert!(text.contains("REMOTE:       172.67.10.20:443"));
        assert!(text.contains("DOMAIN:       example.com"));
        assert!(text.contains("CIDR:         172.64.0.0/13"));
        assert!(text.contains("NETNAME:      CLOUDFLARENET"));
        assert!(text.contains("ASN:          AS13335"));
        assert!(text.contains("ORGANIZATION: Cloudflare, Inc."));
        assert!(text.contains("STATUS:       CLOSED"));
    }

    #[tokio::test]
    async fn tsv_header_and_escaping_round_trip() {
        let db = Database::open_in_memory().await.expect("open");
        let process_id = insert_process(
            db.pool(),
            &NewProcess {
                pid: 7,
                process_name: Some("has\ttab".into()),
                executable_path: None,
                process_start_time: t(2026, 1, 1, 0, 0, 0),
                first_seen_at: t(2026, 1, 1, 0, 0, 0),
                last_seen_at: t(2026, 1, 1, 0, 0, 0),
            },
        )
        .await
        .expect("process");
        insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "udp".into(),
                direction: "inbound".into(),
                local_ip: "127.0.0.1".into(),
                local_port: 53,
                remote_ip: "8.8.8.8".into(),
                remote_port: 53,
                started_at: t(2026, 1, 2, 3, 4, 5),
                ended_at: None,
                bytes_sent: None,
                bytes_received: None,
                collector_id: None,
            },
        )
        .await
        .expect("conn");

        let mut out = Vec::new();
        export_connection_history(
            db.pool(),
            &ExportOptions {
                format: ExportFormat::Tsv,
                export_date: t(2026, 1, 3, 0, 0, 0),
                chunk_size: 50,
                ..ExportOptions::default()
            },
            &mut out,
        )
        .await
        .expect("export");

        let text = String::from_utf8(out).expect("utf8");
        let mut lines = text.lines();
        let header = lines.next().expect("header");
        assert_eq!(header, TSV_COLUMNS.join("\t"));
        let row = lines.next().expect("row");
        assert!(row.contains("\"has\ttab\""), "expected escaped tab field: {row}");
        assert!(row.contains("UDP"));
        assert!(row.contains("INBOUND"));
        assert!(row.contains("ACTIVE"));
    }

    #[tokio::test]
    async fn multi_thousand_row_export_streams_in_chunks() {
        let db = Database::open_in_memory().await.expect("open");
        let process_id = insert_process(
            db.pool(),
            &NewProcess {
                pid: 1,
                process_name: Some("bulk".into()),
                executable_path: None,
                process_start_time: t(2025, 1, 1, 0, 0, 0),
                first_seen_at: t(2025, 1, 1, 0, 0, 0),
                last_seen_at: t(2025, 1, 1, 0, 0, 0),
            },
        )
        .await
        .expect("process");

        const TOTAL: usize = 3_000;
        for i in 0..TOTAL {
            let started = t(2025, 1, 1, 0, 0, 0) + chrono::Duration::seconds(i as i64);
            insert_connection(
                db.pool(),
                &NewConnection {
                    process_id,
                    protocol: "tcp".into(),
                    direction: "outbound".into(),
                    local_ip: "10.0.0.1".into(),
                    local_port: (10_000 + (i % 20_000)) as u16,
                    remote_ip: format!("203.0.113.{}", i % 200),
                    remote_port: 443,
                    started_at: started,
                    ended_at: None,
                    bytes_sent: Some(1),
                    bytes_received: Some(1),
                    collector_id: None,
                },
            )
            .await
            .expect("insert");
        }

        // Cap a single write well below the size of the full export so a
        // one-shot String dump would fail this assertion.
        let mut probe = ChunkProbeWriter::new(8 * 1024);
        let writes = Rc::clone(&probe.writes);
        let max_seen = Rc::clone(&probe.max_seen);
        let bytes = Rc::clone(&probe.bytes);

        let summary = export_connection_history(
            db.pool(),
            &ExportOptions {
                format: ExportFormat::Tsv,
                export_date: t(2025, 2, 1, 0, 0, 0),
                chunk_size: 100,
                ..ExportOptions::default()
            },
            &mut probe,
        )
        .await
        .expect("export");

        assert_eq!(summary.rows_written, TOTAL as u64);
        assert!(
            writes.get() > 10,
            "expected many chunked writes, got {}",
            writes.get()
        );
        assert!(
            max_seen.get() <= 8 * 1024,
            "max write {} exceeded chunk budget",
            max_seen.get()
        );
        assert!(bytes.get() > TOTAL * 20);

        let text = String::from_utf8(probe.data).expect("utf8");
        // Header + 3000 data rows.
        assert_eq!(text.lines().count(), TOTAL + 1);

        // Selected-ids path also streams without loading everything first.
        let selected: Vec<i64> = sqlx::query_scalar("SELECT id FROM connections ORDER BY id LIMIT 5")
            .fetch_all(db.pool())
            .await
            .expect("ids");
        let mut selected_out = Vec::new();
        let selected_summary = export_connection_history(
            db.pool(),
            &ExportOptions {
                selected_ids: Some(selected),
                format: ExportFormat::Txt,
                export_date: t(2025, 2, 1, 0, 0, 0),
                chunk_size: 2,
                ..ExportOptions::default()
            },
            &mut selected_out,
        )
        .await
        .expect("selected export");
        assert_eq!(selected_summary.rows_written, 5);
        let selected_text = String::from_utf8(selected_out).expect("utf8");
        assert!(selected_text.contains("Filter: Selected = 5 row(s)"));
    }

    #[tokio::test]
    async fn export_to_path_writes_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out.tsv");
        let db = Database::open_in_memory().await.expect("open");
        let process_id = insert_process(
            db.pool(),
            &NewProcess {
                pid: 9,
                process_name: Some("curl".into()),
                executable_path: None,
                process_start_time: t(2024, 1, 1, 0, 0, 0),
                first_seen_at: t(2024, 1, 1, 0, 0, 0),
                last_seen_at: t(2024, 1, 1, 0, 0, 0),
            },
        )
        .await
        .expect("process");
        insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "127.0.0.1".into(),
                local_port: 1,
                remote_ip: "1.1.1.1".into(),
                remote_port: 443,
                started_at: t(2024, 1, 2, 0, 0, 0),
                ended_at: None,
                bytes_sent: None,
                bytes_received: None,
                collector_id: None,
            },
        )
        .await
        .expect("conn");

        let summary = export_connection_history_to_path(
            db.pool(),
            &ExportOptions {
                format: ExportFormat::Tsv,
                export_date: t(2024, 1, 3, 0, 0, 0),
                chunk_size: 50,
                ..ExportOptions::default()
            },
            &path,
        )
        .await
        .expect("path export");
        assert_eq!(summary.rows_written, 1);
        let contents = std::fs::read_to_string(&path).expect("read");
        assert!(contents.starts_with(&TSV_COLUMNS.join("\t")));
        assert!(contents.contains("curl"));
    }
}
