//! Asynchronous enrichment worker: queues lookups off the collector path.

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;

use chrono::Utc;
use database::{
    get_bool_setting, get_ip_winning_network, list_networks, store_winning_ip_network,
    upsert_ip_address, Database, SETTING_IP_ENRICHMENT,
};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::classify::requires_public_lookup;
use crate::http::HttpClient;
use crate::match_prefix::{longest_prefix_match, NetworkPrefix};
use crate::rdap::{RdapClient, RdapClientConfig};
use crate::EnrichmentError;

/// Configuration for the enrichment worker.
#[derive(Debug, Clone)]
pub struct EnrichmentConfig {
    /// When false, the worker never performs HTTP (also re-checked via DB setting).
    pub enabled: bool,
    /// Channel capacity for pending IP lookups.
    pub queue_capacity: usize,
    /// RDAP HTTP behavior.
    pub rdap: RdapClientConfig,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            queue_capacity: 1024,
            rdap: RdapClientConfig::default(),
        }
    }
}

/// Non-blocking handle used by the collector / pipeline to request enrichment.
#[derive(Clone, Debug)]
pub struct EnrichmentHandle {
    tx: mpsc::Sender<EnrichmentRequest>,
    /// True when the handle was created with enrichment disabled at construction.
    construction_enabled: bool,
}

#[derive(Debug)]
struct EnrichmentRequest {
    ip: IpAddr,
    /// Optional completion signal for tests that need to await persistence.
    done: Option<oneshot::Sender<()>>,
}

impl EnrichmentHandle {
    /// Queues `ip` for enrichment without waiting for HTTP or persistence.
    ///
    /// Returns immediately. Special-use addresses and a disabled construction
    /// flag never enter the queue. A full channel drops the request rather than
    /// blocking the caller.
    pub fn enqueue(&self, ip: IpAddr) -> bool {
        if !self.construction_enabled {
            return false;
        }
        if !requires_public_lookup(ip) {
            return false;
        }
        self.tx
            .try_send(EnrichmentRequest { ip, done: None })
            .is_ok()
    }

    /// Like [`enqueue`] but notifies when the worker finishes processing this IP.
    pub async fn enqueue_and_wait(&self, ip: IpAddr) -> bool {
        if !self.construction_enabled || !requires_public_lookup(ip) {
            return false;
        }
        let (done_tx, done_rx) = oneshot::channel();
        if self
            .tx
            .send(EnrichmentRequest {
                ip,
                done: Some(done_tx),
            })
            .await
            .is_err()
        {
            return false;
        }
        let _ = done_rx.await;
        true
    }
}

/// Running enrichment worker (HTTP + SQLite persistence).
pub struct EnrichmentWorker {
    handle: EnrichmentHandle,
    join: tokio::task::JoinHandle<()>,
}

impl EnrichmentWorker {
    /// Spawns a background worker that drains the enrichment queue.
    pub fn spawn<C>(db: Database, http: Arc<C>, config: EnrichmentConfig) -> Self
    where
        C: HttpClient + 'static,
    {
        let construction_enabled = config.enabled;
        let (tx, rx) = mpsc::channel(config.queue_capacity);
        let handle = EnrichmentHandle {
            tx,
            construction_enabled,
        };

        let join = tokio::spawn(async move {
            worker_loop(db, http, config, rx).await;
        });

        Self { handle, join }
    }

    /// Cloneable enqueue handle (never blocks on HTTP).
    pub fn handle(&self) -> EnrichmentHandle {
        self.handle.clone()
    }

    /// Stops accepting work and waits for the worker task to exit.
    ///
    /// Aborts the task so shutdown cannot hang on outstanding handle clones.
    pub async fn shutdown(self) {
        drop(self.handle);
        self.join.abort();
        let _ = self.join.await;
    }
}

async fn worker_loop<C>(
    db: Database,
    http: Arc<C>,
    config: EnrichmentConfig,
    mut rx: mpsc::Receiver<EnrichmentRequest>,
) where
    C: HttpClient + 'static,
{
    let rdap = RdapClient::new(http, config.rdap.clone());
    let mut in_memory_done: HashSet<IpAddr> = HashSet::new();
    let mut in_flight: HashSet<IpAddr> = HashSet::new();

    while let Some(request) = rx.recv().await {
        let EnrichmentRequest { ip, done } = request;

        if !requires_public_lookup(ip) {
            signal_done(done);
            continue;
        }

        if in_memory_done.contains(&ip) || in_flight.contains(&ip) {
            signal_done(done);
            continue;
        }

        if !config.enabled {
            signal_done(done);
            continue;
        }

        let enabled = match get_bool_setting(db.pool(), SETTING_IP_ENRICHMENT, true).await {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "failed to read ip_enrichment setting");
                signal_done(done);
                continue;
            }
        };
        if !enabled {
            signal_done(done);
            continue;
        }

        // Cache hit: IP already associated with a stored network — no HTTP.
        if let Ok(Some(_)) = cached_network_for_ip(&db, ip).await {
            in_memory_done.insert(ip);
            signal_done(done);
            continue;
        }

        // Longest-prefix match against already-known networks (no HTTP).
        if let Ok(Some(prefix_cidr)) = match_existing_prefix(&db, ip).await {
            if let Err(err) = associate_existing_prefix(&db, ip, &prefix_cidr).await {
                warn!(error = %err, %ip, "failed to associate existing network prefix");
            } else {
                in_memory_done.insert(ip);
            }
            signal_done(done);
            continue;
        }

        in_flight.insert(ip);
        match rdap.lookup_ip(ip).await {
            Ok(info) => {
                let network = info.into_new_network(Utc::now());
                if let Err(err) =
                    store_winning_ip_network(db.pool(), &ip.to_string(), &network, Utc::now())
                        .await
                {
                    warn!(error = %err, %ip, "failed to persist RDAP network");
                } else {
                    in_memory_done.insert(ip);
                }
            }
            Err(err) => {
                warn!(error = %err, %ip, "RDAP lookup failed");
                // Remember the attempt so a second sighting does not hammer HTTP.
                in_memory_done.insert(ip);
            }
        }
        in_flight.remove(&ip);
        signal_done(done);
    }
}

fn signal_done(done: Option<oneshot::Sender<()>>) {
    if let Some(tx) = done {
        let _ = tx.send(());
    }
}

async fn cached_network_for_ip(
    db: &Database,
    ip: IpAddr,
) -> Result<Option<()>, EnrichmentError> {
    let ip_str = ip.to_string();
    // Ensure the IP row exists for association lookups; cheap upsert of last_seen.
    let ip_id = upsert_ip_address(db.pool(), &ip_str, Utc::now()).await?;
    let winning = get_ip_winning_network(db.pool(), ip_id).await?;
    Ok(winning.map(|_| ()))
}

async fn match_existing_prefix(db: &Database, ip: IpAddr) -> Result<Option<String>, EnrichmentError> {
    let rows = list_networks(db.pool()).await?;
    let prefixes: Vec<NetworkPrefix> = rows
        .iter()
        .filter_map(|row| NetworkPrefix::parse(&row.cidr).ok())
        .collect();
    Ok(longest_prefix_match(ip, &prefixes).map(|p| p.to_string()))
}

async fn associate_existing_prefix(
    db: &Database,
    ip: IpAddr,
    cidr: &str,
) -> Result<(), EnrichmentError> {
    let rows = list_networks(db.pool()).await?;
    let network = rows
        .into_iter()
        .find(|row| row.cidr == cidr)
        .ok_or_else(|| EnrichmentError::Parse(format!("missing network row for {cidr}")))?;

    let network_input = database::NewNetwork {
        cidr: network.cidr,
        net_range: network.net_range,
        net_name: network.net_name,
        organization: network.organization,
        asn: network.asn,
        country: network.country,
        source: network.source,
        updated_at: Utc::now(),
    };
    store_winning_ip_network(db.pool(), &ip.to_string(), &network_input, Utc::now()).await?;
    Ok(())
}

/// Test helper: counts HTTP GETs and optionally stalls until released.
#[cfg(test)]
pub struct MockHttpClient {
    pub calls: std::sync::atomic::AtomicUsize,
    pub body: tokio::sync::Mutex<String>,
    /// When set, each GET waits until [`tokio::sync::Notify::notify_one`] is called.
    pub gate: Option<Arc<tokio::sync::Notify>>,
    pub fail_times: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl MockHttpClient {
    pub fn with_body(body: impl Into<String>) -> Self {
        Self {
            calls: std::sync::atomic::AtomicUsize::new(0),
            body: tokio::sync::Mutex::new(body.into()),
            gate: None,
            fail_times: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn with_gate(body: impl Into<String>, gate: Arc<tokio::sync::Notify>) -> Self {
        Self {
            calls: std::sync::atomic::AtomicUsize::new(0),
            body: tokio::sync::Mutex::new(body.into()),
            gate: Some(gate),
            fail_times: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl HttpClient for MockHttpClient {
    async fn get_text(&self, _url: &str) -> Result<String, EnrichmentError> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        let remaining = self
            .fail_times
            .load(std::sync::atomic::Ordering::SeqCst);
        if remaining > 0 {
            self.fail_times
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            return Err(EnrichmentError::Http("mock failure".into()));
        }
        Ok(self.body.lock().await.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rdap::cloudflare_rdap_fixture;
    use database::{
        get_ip_winning_network, set_bool_setting, upsert_ip_address, Database, SETTING_IP_ENRICHMENT,
    };
    use std::str::FromStr;
    use std::time::Duration;
    use tokio::sync::Notify;
    use tokio::time::timeout;

    fn fast_config(enabled: bool) -> EnrichmentConfig {
        EnrichmentConfig {
            enabled,
            queue_capacity: 16,
            rdap: RdapClientConfig {
                url_template: "https://example.test/rdap/ip/{ip}".into(),
                timeout: Duration::from_secs(2),
                max_retries: 0,
                retry_delay: Duration::from_millis(1),
                min_interval: Duration::from_millis(0),
            },
        }
    }

    async fn winning_for(db: &Database, ip: &str) -> database::NetworkRow {
        let ip_id = upsert_ip_address(db.pool(), ip, Utc::now())
            .await
            .expect("upsert ip");
        get_ip_winning_network(db.pool(), ip_id)
            .await
            .expect("get")
            .expect("network")
    }

    #[tokio::test]
    async fn mock_cloudflare_fields_are_persisted() {
        let db = Database::open_in_memory().await.expect("db");
        let http = Arc::new(MockHttpClient::with_body(cloudflare_rdap_fixture()));
        let worker = EnrichmentWorker::spawn(db.clone(), http.clone(), fast_config(true));

        let ip = IpAddr::from_str("172.67.10.20").unwrap();
        assert!(worker.handle().enqueue_and_wait(ip).await);
        assert_eq!(http.call_count(), 1);

        let network = winning_for(&db, "172.67.10.20").await;
        assert_eq!(network.cidr, "172.64.0.0/13");
        assert_eq!(
            network.net_range.as_deref(),
            Some("172.64.0.0 - 172.71.255.255")
        );
        assert_eq!(network.net_name.as_deref(), Some("CLOUDFLARENET"));
        assert_eq!(network.organization.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(network.asn, Some(13335));
        assert_eq!(network.country.as_deref(), Some("US"));
        assert_eq!(network.source.as_deref(), Some("rdap"));

        worker.shutdown().await;
    }

    #[tokio::test]
    async fn second_sighting_does_not_call_http_again() {
        let db = Database::open_in_memory().await.expect("db");
        let http = Arc::new(MockHttpClient::with_body(cloudflare_rdap_fixture()));
        let worker = EnrichmentWorker::spawn(db.clone(), http.clone(), fast_config(true));
        let handle = worker.handle();

        let ip = IpAddr::from_str("172.67.10.20").unwrap();
        assert!(handle.enqueue_and_wait(ip).await);
        assert_eq!(http.call_count(), 1);

        assert!(handle.enqueue_and_wait(ip).await);
        assert_eq!(http.call_count(), 1, "cached IP must not hit HTTP again");

        worker.shutdown().await;
    }

    #[tokio::test]
    async fn special_use_ips_never_hit_http() {
        let db = Database::open_in_memory().await.expect("db");
        let http = Arc::new(MockHttpClient::with_body(cloudflare_rdap_fixture()));
        let worker = EnrichmentWorker::spawn(db.clone(), http.clone(), fast_config(true));
        let handle = worker.handle();

        for ip in [
            "10.0.0.1",
            "192.168.1.1",
            "127.0.0.1",
            "169.254.10.20",
            "224.0.0.1",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
        ] {
            let addr = IpAddr::from_str(ip).unwrap();
            assert!(!handle.enqueue(addr), "{ip} must not enqueue");
            assert!(!handle.enqueue_and_wait(addr).await, "{ip} must not wait-enqueue");
        }

        // Give the worker a moment; nothing should have been queued.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(http.call_count(), 0);
        worker.shutdown().await;
    }

    #[tokio::test]
    async fn disabled_setting_performs_no_http() {
        let db = Database::open_in_memory().await.expect("db");
        set_bool_setting(db.pool(), SETTING_IP_ENRICHMENT, false)
            .await
            .expect("disable");

        let http = Arc::new(MockHttpClient::with_body(cloudflare_rdap_fixture()));
        // Construction enabled so requests reach the worker, which re-checks the DB setting.
        let worker = EnrichmentWorker::spawn(db.clone(), http.clone(), fast_config(true));
        let ip = IpAddr::from_str("1.1.1.1").unwrap();
        assert!(worker.handle().enqueue_and_wait(ip).await);
        assert_eq!(http.call_count(), 0);

        // Construction-time disable also skips HTTP without enqueueing.
        let worker2 = EnrichmentWorker::spawn(db.clone(), http.clone(), fast_config(false));
        assert!(!worker2.handle().enqueue(ip));
        assert_eq!(http.call_count(), 0);

        worker.shutdown().await;
        worker2.shutdown().await;
    }

    #[tokio::test]
    async fn pipeline_finishes_while_lookup_pending() {
        let db = Database::open_in_memory().await.expect("db");
        let gate = Arc::new(Notify::new());
        let http = Arc::new(MockHttpClient::with_gate(
            cloudflare_rdap_fixture(),
            gate.clone(),
        ));
        let worker = EnrichmentWorker::spawn(db.clone(), http.clone(), fast_config(true));
        let handle = worker.handle();

        let ip = IpAddr::from_str("172.67.10.20").unwrap();
        // Non-blocking enqueue — must return before HTTP completes.
        assert!(handle.enqueue(ip));

        // Simulate collector / pipeline finishing its critical path while RDAP is pending.
        let pipeline_finished = async {
            upsert_ip_address(db.pool(), "172.67.10.20", Utc::now())
                .await
                .expect("pipeline upsert");
            // Connection-side work can complete independently of enrichment.
            "pipeline-ok"
        };

        let result = timeout(Duration::from_millis(500), pipeline_finished)
            .await
            .expect("pipeline must not wait on RDAP");
        assert_eq!(result, "pipeline-ok");

        // Wait until the mock HTTP call has started (still blocked on the gate).
        timeout(Duration::from_secs(2), async {
            while http.call_count() == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("HTTP should start while pipeline already finished");
        assert_eq!(http.call_count(), 1);

        let ip_id = upsert_ip_address(db.pool(), "172.67.10.20", Utc::now())
            .await
            .unwrap();
        assert!(
            get_ip_winning_network(db.pool(), ip_id)
                .await
                .unwrap()
                .is_none(),
            "enrichment must still be pending"
        );

        // Release the mock HTTP and wait for persistence.
        gate.notify_one();
        let wait = timeout(Duration::from_secs(2), async {
            loop {
                let id = upsert_ip_address(db.pool(), "172.67.10.20", Utc::now())
                    .await
                    .unwrap();
                if get_ip_winning_network(db.pool(), id)
                    .await
                    .unwrap()
                    .is_some()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(wait.is_ok(), "enrichment should complete after gate release");

        worker.shutdown().await;
    }
}
