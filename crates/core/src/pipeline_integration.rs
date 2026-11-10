//! End-to-end pipeline: MockCollector → core → RDAP/DNS fakes → SQLite → query → stats → TXT.

use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use collector_api::NetworkCollector;
use common::{ConnectionEvent, ConnectionEventType, Direction, IpAddress, Protocol};
use database::{
    query_connection_history, ConnectionHistoryFilter, ConnectionHistoryQuery, Database,
    ExportFormat, ExportOptions,
};
use dns_resolver::{DnsConfig, DnsError, DnsWorker, PtrResolver};
use ip_enrichment::{
    EnrichmentConfig, EnrichmentError, EnrichmentWorker, HttpClient, RdapClientConfig,
};
use statistics::{process_stats, DateRange};
use tokio::sync::mpsc;

use crate::{EventPipeline, MockCollector};

fn rdap_fixture() -> String {
    r#"{
        "objectClassName": "ip network",
        "handle": "NET-172-64-0-0-1",
        "startAddress": "172.64.0.0",
        "endAddress": "172.71.255.255",
        "name": "CLOUDFLARENET",
        "country": "US",
        "cidr0_cidrs": [{ "v4prefix": "172.64.0.0", "length": 13 }],
        "entities": [{
            "objectClassName": "entity",
            "handle": "CLOUD14",
            "roles": ["registrant"],
            "vcardArray": ["vcard", [["fn", {}, "text", "Cloudflare, Inc."]]],
            "asn": "AS13335"
        }]
    }"#
    .to_owned()
}

struct FakeHttpClient {
    body: String,
}

#[async_trait]
impl HttpClient for FakeHttpClient {
    async fn get_text(&self, _url: &str) -> Result<String, EnrichmentError> {
        Ok(self.body.clone())
    }
}

struct FakePtrResolver {
    names: Vec<String>,
}

#[async_trait]
impl PtrResolver for FakePtrResolver {
    async fn lookup_ptr(&self, _ip: IpAddr) -> Result<Vec<String>, DnsError> {
        Ok(self.names.clone())
    }
}

fn sample_events() -> Vec<ConnectionEvent> {
    let process_start = Utc.with_ymd_and_hms(2026, 11, 9, 10, 0, 0).unwrap();
    let opened_at = Utc.with_ymd_and_hms(2026, 11, 9, 10, 0, 5).unwrap();
    let closed_at = Utc.with_ymd_and_hms(2026, 11, 9, 10, 0, 30).unwrap();

    let base = |event_type: ConnectionEventType, timestamp| ConnectionEvent {
        timestamp,
        event_type,
        process_id: 4242,
        process_name: Some("curl".into()),
        process_path: Some("/usr/bin/curl".into()),
        process_start_time: Some(process_start),
        protocol: Protocol::Tcp,
        local_ip: IpAddress::parse("192.168.1.10").unwrap(),
        local_port: 54321,
        remote_ip: IpAddress::parse("172.67.10.20").unwrap(),
        remote_port: 443,
        direction: Direction::Outbound,
        bytes_sent: Some(128),
        bytes_received: Some(4096),
        remote_hostname: None,
    };

    vec![
        base(ConnectionEventType::NewConnection, opened_at),
        base(ConnectionEventType::ConnectionClosed, closed_at),
    ]
}

#[tokio::test]
async fn mock_collector_through_export_pipeline() {
    let db = Database::open_in_memory().await.expect("db");

    let enrichment = EnrichmentWorker::spawn(
        db.clone(),
        Arc::new(FakeHttpClient {
            body: rdap_fixture(),
        }),
        EnrichmentConfig {
            enabled: true,
            queue_capacity: 16,
            rdap: RdapClientConfig {
                url_template: "https://example.test/rdap/ip/{ip}".into(),
                timeout: Duration::from_secs(2),
                max_retries: 0,
                retry_delay: Duration::from_millis(1),
                min_interval: Duration::from_millis(0),
            },
        },
    );
    let dns = DnsWorker::spawn(
        db.clone(),
        Arc::new(FakePtrResolver {
            names: vec!["CDN.Example.NET.".into()],
        }),
        DnsConfig {
            enabled: true,
            queue_capacity: 16,
            timeout: Duration::from_secs(2),
        },
    );

    let pipeline = EventPipeline::new(db.clone()).with_collector_id("mock");
    let mut collector = MockCollector::new(sample_events());
    let (tx, rx) = mpsc::channel(8);
    collector.start(tx).await.expect("collector start");
    pipeline.run(rx).await.expect("pipeline");
    collector.stop().await.expect("collector stop");

    let remote = IpAddr::from_str("172.67.10.20").expect("ip");
    assert!(
        enrichment.handle().enqueue_and_wait(remote).await,
        "RDAP enrichment should accept the remote IP"
    );
    assert!(
        dns.handle().enqueue_and_wait(remote).await,
        "DNS enrichment should accept the remote IP"
    );

    let page = query_connection_history(db.pool(), &ConnectionHistoryQuery::default())
        .await
        .expect("history query");
    assert_eq!(page.total, 1);
    assert_eq!(page.rows.len(), 1);
    let row = &page.rows[0];
    assert_eq!(row.process_name.as_deref(), Some("curl"));
    assert_eq!(row.remote_ip, "172.67.10.20");
    assert_eq!(row.domain.as_deref(), Some("cdn.example.net"));
    assert_eq!(row.network_cidr.as_deref(), Some("172.64.0.0/13"));
    assert_eq!(row.network_name.as_deref(), Some("CLOUDFLARENET"));
    assert_eq!(row.organization.as_deref(), Some("Cloudflare, Inc."));
    assert!(row.ended_at.is_some());

    let stats = process_stats(
        &db,
        DateRange {
            from: Utc.with_ymd_and_hms(2026, 11, 9, 0, 0, 0).unwrap(),
            to: Utc.with_ymd_and_hms(2026, 11, 9, 23, 59, 59).unwrap(),
        },
    )
    .await
    .expect("process stats");
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].process, "curl");
    assert_eq!(stats[0].connections, 1);
    assert_eq!(stats[0].unique_ips, 1);
    assert!(stats[0].domains >= 1);

    let mut export_buf = Vec::new();
    let summary = db
        .export_connection_history(
            &ExportOptions {
                filter: ConnectionHistoryFilter::default(),
                selected_ids: None,
                format: ExportFormat::Txt,
                export_date: Utc.with_ymd_and_hms(2026, 11, 9, 12, 0, 0).unwrap(),
                chunk_size: 50,
            },
            &mut export_buf,
        )
        .await
        .expect("txt export");
    assert_eq!(summary.rows_written, 1);
    let export_text = String::from_utf8(export_buf).expect("utf8");
    assert!(
        export_text.contains("curl"),
        "export missing process: {export_text}"
    );
    assert!(
        export_text.contains("172.67.10.20"),
        "export missing remote IP: {export_text}"
    );
    assert!(
        export_text.contains("cdn.example.net"),
        "export missing domain: {export_text}"
    );
    assert!(
        export_text.contains("CLOUDFLARENET") || export_text.contains("172.64.0.0/13"),
        "export missing network enrichment: {export_text}"
    );

    enrichment.shutdown().await;
    dns.shutdown().await;
}
