use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use collector_api::{CollectorCapabilities, NetworkCollector};
use common::{ConnectionEventType, Direction, IpAddress, Protocol};
use process_info::{FakeProcessSource, ProcessInfo};

use crate::diff::DiffEngine;
use crate::snapshot::SocketObservation;
use crate::{
    macos_capabilities, FixtureSnapshotSource, MacOsCollector, CRATE_NAME, POLL_INTERVAL_MS,
    UDP_IDLE_CLOSE_SECS,
};

fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + secs, 0).unwrap()
}

fn tcp(pid: u32, local_port: u16, remote_port: u16) -> SocketObservation {
    SocketObservation {
        pid,
        protocol: Protocol::Tcp,
        local_ip: IpAddress::parse("192.168.1.10").unwrap(),
        local_port,
        remote_ip: IpAddress::parse("192.0.2.10").unwrap(),
        remote_port,
        direction: Direction::Outbound,
    }
}

fn udp(pid: u32, local_port: u16, remote_port: u16) -> SocketObservation {
    SocketObservation {
        pid,
        protocol: Protocol::Udp,
        local_ip: IpAddress::parse("192.168.1.10").unwrap(),
        local_port,
        remote_ip: IpAddress::parse("192.0.2.53").unwrap(),
        remote_port,
        direction: Direction::Outbound,
    }
}

fn processes_with(pid: u32) -> FakeProcessSource {
    let mut source = FakeProcessSource::new();
    source.insert(ProcessInfo {
        pid,
        name: Some("fixture-app".into()),
        executable_path: Some("/Applications/Fixture.app/Contents/MacOS/fixture".into()),
        process_start_time: Some(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
    });
    source
}

#[test]
fn crate_name_is_collector_macos() {
    assert_eq!(CRATE_NAME, "collector-macos");
}

#[test]
fn new_tcp_from_empty_to_present_snapshot() {
    let mut engine = DiffEngine::default();
    let processes = processes_with(4242);
    let sock = tcp(4242, 54321, 443);

    let events = engine.apply(&[sock.clone()], ts(0), &processes);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, ConnectionEventType::NewConnection);
    assert_eq!(events[0].protocol, Protocol::Tcp);
    assert_eq!(events[0].process_id, 4242);
    assert_eq!(events[0].local_port, 54321);
    assert_eq!(events[0].remote_port, 443);
    assert!(events[0].bytes_sent.is_none());
    assert!(events[0].bytes_received.is_none());
}

#[test]
fn closed_tcp_from_present_to_absent_snapshot() {
    let mut engine = DiffEngine::default();
    let processes = processes_with(100);
    let sock = tcp(100, 40000, 443);

    let open = engine.apply(std::slice::from_ref(&sock), ts(0), &processes);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].event_type, ConnectionEventType::NewConnection);

    let closed = engine.apply(&[], ts(1), &processes);
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].event_type, ConnectionEventType::ConnectionClosed);
    assert_eq!(closed[0].protocol, Protocol::Tcp);
    assert_eq!(closed[0].local_port, 40000);
    assert_eq!(closed[0].remote_port, 443);
}

#[test]
fn udp_idle_expiry_after_30s_without_sighting() {
    let mut engine = DiffEngine::default();
    let processes = processes_with(50);
    let sock = udp(50, 53000, 53);

    let open = engine.apply(std::slice::from_ref(&sock), ts(0), &processes);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].event_type, ConnectionEventType::NewConnection);
    assert_eq!(open[0].protocol, Protocol::Udp);

    // Still within the idle window — no close.
    let mid = engine.apply(&[], ts(29), &processes);
    assert!(mid.is_empty());

    // At/after 30s without sighting — close.
    let expired = engine.apply(&[], ts(30), &processes);
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].event_type, ConnectionEventType::ConnectionClosed);
    assert_eq!(expired[0].protocol, Protocol::Udp);
}

#[test]
fn udp_reobserved_before_idle_keeps_flow_open() {
    let mut engine = DiffEngine::default();
    let processes = processes_with(50);
    let sock = udp(50, 53000, 53);

    assert_eq!(
        engine.apply(std::slice::from_ref(&sock), ts(0), &processes).len(),
        1
    );
    // Absent briefly…
    assert!(engine.apply(&[], ts(20), &processes).is_empty());
    // …then sighted again, resetting the idle timer.
    assert!(engine
        .apply(std::slice::from_ref(&sock), ts(25), &processes)
        .is_empty());
    // 20s after the refresh is still open.
    assert!(engine.apply(&[], ts(45), &processes).is_empty());
    // 30s after the refresh closes.
    let closed = engine.apply(&[], ts(55), &processes);
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].event_type, ConnectionEventType::ConnectionClosed);
}

#[test]
fn duplicate_identical_snapshot_emits_nothing() {
    let mut engine = DiffEngine::default();
    let processes = processes_with(7);
    let sock = tcp(7, 12345, 443);
    let snapshot = vec![sock];

    let first = engine.apply(&snapshot, ts(0), &processes);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].event_type, ConnectionEventType::NewConnection);

    let second = engine.apply(&snapshot, ts(1), &processes);
    assert!(second.is_empty());

    let third = engine.apply(&snapshot, ts(2), &processes);
    assert!(third.is_empty());
}

#[test]
fn inaccessible_process_leaves_path_null() {
    let mut engine = DiffEngine::default();
    // Empty FakeProcessSource → unknown PID fields stay None (never guessed).
    let processes = FakeProcessSource::new();
    let sock = tcp(999_999, 55555, 443);

    let events = engine.apply(&[sock], ts(0), &processes);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].process_id, 999_999);
    assert_eq!(events[0].process_name, None);
    assert_eq!(events[0].process_path, None);
    assert_eq!(events[0].process_start_time, None);
}

#[test]
fn capabilities_match_what_the_poller_sets() {
    let caps = macos_capabilities();
    assert!(caps.new_connection_detection);
    assert!(caps.existing_connection_enumeration);
    assert!(caps.connection_termination_detection);
    assert!(caps.tcp);
    assert!(caps.udp);
    assert!(!caps.traffic_byte_counters);
    assert!(caps.process_identification);
    assert!(caps.incoming_connection_detection);

    let collector =
        MacOsCollector::with_sources(FixtureSnapshotSource::new(Vec::new()), FakeProcessSource::new());
    assert_eq!(collector.platform(), collector_api::Platform::MacOs);
    assert_eq!(collector.capabilities(), caps);
    assert_eq!(collector.capabilities(), CollectorCapabilities {
        new_connection_detection: true,
        existing_connection_enumeration: true,
        connection_termination_detection: true,
        tcp: true,
        udp: true,
        traffic_byte_counters: false,
        process_identification: true,
        incoming_connection_detection: true,
    });
    assert_eq!(collector.poll_interval().as_millis() as u64, POLL_INTERVAL_MS);
    assert_eq!(collector.udp_idle_timeout().as_secs(), UDP_IDLE_CLOSE_SECS);
    assert_eq!(
        DiffEngine::default().udp_idle(),
        std::time::Duration::from_secs(UDP_IDLE_CLOSE_SECS)
    );
}

#[tokio::test]
async fn fixture_collector_start_stop_and_emits_new_tcp() {
    use tokio::sync::mpsc;

    let sock = tcp(42, 40001, 443);
    let source = FixtureSnapshotSource::new(vec![sock]);
    let processes = processes_with(42);
    let mut collector = MacOsCollector::with_sources(source, processes);
    let (tx, mut rx) = mpsc::channel(8);

    collector.start(tx).await.expect("start");
    assert!(collector.is_running());

    let event = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
        .await
        .expect("timed out waiting for event")
        .expect("channel closed");
    assert_eq!(event.event_type, ConnectionEventType::NewConnection);
    assert_eq!(event.process_id, 42);
    assert_eq!(event.remote_port, 443);

    // Duplicate snapshot ticks should not spam events; drain briefly.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    while let Ok(extra) = rx.try_recv() {
        panic!("unexpected extra event: {extra:?}");
    }

    collector.stop().await.expect("stop");
    assert!(!collector.is_running());
}

#[test]
fn udp_idle_constant_matches_chrono_window() {
    // Guard against accidental skew between Duration and ChronoDuration usage.
    assert_eq!(UDP_IDLE_CLOSE_SECS, 30);
    assert_eq!(
        ChronoDuration::seconds(UDP_IDLE_CLOSE_SECS as i64).num_seconds(),
        30
    );
}
