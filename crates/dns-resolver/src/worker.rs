//! Asynchronous reverse-DNS worker: queues PTR lookups off the collector path.

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use common::normalize_domain;
use database::{upsert_ip_domain, Database, NewIpDomain};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::ptr::PtrResolver;
use crate::{DomainSource, DnsError, REVERSE_DNS_CONFIDENCE};

/// Configuration for the reverse-DNS worker.
#[derive(Debug, Clone)]
pub struct DnsConfig {
    /// When false, the worker never performs PTR lookups.
    pub enabled: bool,
    /// Channel capacity for pending IP lookups.
    pub queue_capacity: usize,
    /// Per-lookup timeout applied by callers wrapping the resolver (also used by hickory impl).
    pub timeout: Duration,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            queue_capacity: 1024,
            timeout: Duration::from_secs(5),
        }
    }
}

/// Non-blocking handle used by the collector / pipeline to request reverse DNS.
#[derive(Clone, Debug)]
pub struct DnsHandle {
    tx: mpsc::Sender<DnsRequest>,
    construction_enabled: bool,
}

#[derive(Debug)]
struct DnsRequest {
    ip: IpAddr,
    done: Option<oneshot::Sender<()>>,
}

impl DnsHandle {
    /// Queues `ip` for reverse DNS without waiting for the lookup or persistence.
    ///
    /// Returns immediately. A disabled construction flag or a full channel never blocks
    /// the caller — the request is dropped instead.
    pub fn enqueue(&self, ip: IpAddr) -> bool {
        if !self.construction_enabled {
            return false;
        }
        self.tx
            .try_send(DnsRequest { ip, done: None })
            .is_ok()
    }

    /// Like [`enqueue`] but notifies when the worker finishes processing this IP.
    pub async fn enqueue_and_wait(&self, ip: IpAddr) -> bool {
        if !self.construction_enabled {
            return false;
        }
        let (done_tx, done_rx) = oneshot::channel();
        if self
            .tx
            .send(DnsRequest {
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

/// Running reverse-DNS worker (PTR + SQLite persistence).
pub struct DnsWorker {
    handle: DnsHandle,
    join: tokio::task::JoinHandle<()>,
}

impl DnsWorker {
    /// Spawns a background worker that drains the reverse-DNS queue.
    pub fn spawn<R>(db: Database, resolver: Arc<R>, config: DnsConfig) -> Self
    where
        R: PtrResolver + 'static,
    {
        let construction_enabled = config.enabled;
        let (tx, rx) = mpsc::channel(config.queue_capacity);
        let handle = DnsHandle {
            tx,
            construction_enabled,
        };

        let join = tokio::spawn(async move {
            worker_loop(db, resolver, config, rx).await;
        });

        Self { handle, join }
    }

    /// Cloneable enqueue handle (never blocks on DNS).
    pub fn handle(&self) -> DnsHandle {
        self.handle.clone()
    }

    /// Stops accepting work and aborts the worker task.
    pub async fn shutdown(self) {
        drop(self.handle);
        self.join.abort();
        let _ = self.join.await;
    }
}

async fn worker_loop<R>(
    db: Database,
    resolver: Arc<R>,
    config: DnsConfig,
    mut rx: mpsc::Receiver<DnsRequest>,
) where
    R: PtrResolver + 'static,
{
    // IPs already looked up (success or failure). Cache hits skip the resolver.
    let mut cache: HashSet<IpAddr> = HashSet::new();
    let mut in_flight: HashSet<IpAddr> = HashSet::new();

    while let Some(request) = rx.recv().await {
        let DnsRequest { ip, done } = request;

        if !config.enabled {
            signal_done(done);
            continue;
        }

        if cache.contains(&ip) || in_flight.contains(&ip) {
            signal_done(done);
            continue;
        }

        in_flight.insert(ip);
        match resolve_and_persist(&db, resolver.as_ref(), ip).await {
            Ok(_) => {
                cache.insert(ip);
            }
            Err(err) => {
                warn!(error = %err, %ip, "reverse DNS lookup failed");
                // Cache the negative outcome so a second sighting does not re-hit the resolver.
                // No fake domain is written to the database.
                cache.insert(ip);
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

/// Looks up PTR names, normalizes them, and persists associations.
///
/// Failed lookups and empty responses store nothing. Empty/invalid PTR strings are skipped.
async fn resolve_and_persist<R>(
    db: &Database,
    resolver: &R,
    ip: IpAddr,
) -> Result<Vec<String>, DnsError>
where
    R: PtrResolver,
{
    let raw_names = resolver.lookup_ptr(ip).await?;
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();

    for raw in raw_names {
        let Ok(domain) = normalize_domain(&raw) else {
            continue;
        };
        if seen.insert(domain.clone()) {
            normalized.push(domain);
        }
    }

    let seen_at = Utc::now();
    let source = DomainSource::ReverseDns.as_str();
    for domain in &normalized {
        upsert_ip_domain(
            db.pool(),
            &NewIpDomain {
                ip: ip.to_string(),
                domain: domain.clone(),
                source: source.to_string(),
                confidence: Some(REVERSE_DNS_CONFIDENCE),
                seen_at,
            },
        )
        .await?;
    }

    Ok(normalized)
}

/// Test helper: counts PTR lookups and returns configured names or errors.
#[cfg(test)]
pub struct MockPtrResolver {
    pub calls: std::sync::atomic::AtomicUsize,
    pub names: tokio::sync::Mutex<Result<Vec<String>, String>>,
    pub gate: Option<Arc<tokio::sync::Notify>>,
}

#[cfg(test)]
impl MockPtrResolver {
    pub fn with_names(names: Vec<String>) -> Self {
        Self {
            calls: std::sync::atomic::AtomicUsize::new(0),
            names: tokio::sync::Mutex::new(Ok(names)),
            gate: None,
        }
    }

    pub fn failing(message: impl Into<String>) -> Self {
        Self {
            calls: std::sync::atomic::AtomicUsize::new(0),
            names: tokio::sync::Mutex::new(Err(message.into())),
            gate: None,
        }
    }

    pub fn with_gate(names: Vec<String>, gate: Arc<tokio::sync::Notify>) -> Self {
        Self {
            calls: std::sync::atomic::AtomicUsize::new(0),
            names: tokio::sync::Mutex::new(Ok(names)),
            gate: Some(gate),
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl PtrResolver for MockPtrResolver {
    async fn lookup_ptr(&self, _ip: IpAddr) -> Result<Vec<String>, DnsError> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        match &*self.names.lock().await {
            Ok(names) => Ok(names.clone()),
            Err(msg) => Err(DnsError::Lookup(msg.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{list_domains_for_ip, list_ip_domains, upsert_ip_address, Database};
    use std::str::FromStr;
    use std::time::Duration;
    use tokio::sync::Notify;
    use tokio::time::timeout;

    fn fast_config(enabled: bool) -> DnsConfig {
        DnsConfig {
            enabled,
            queue_capacity: 16,
            timeout: Duration::from_secs(2),
        }
    }

    async fn domains_for(db: &Database, ip: &str) -> Vec<String> {
        let ip_id = upsert_ip_address(db.pool(), ip, Utc::now())
            .await
            .expect("upsert ip");
        list_domains_for_ip(db.pool(), ip_id)
            .await
            .expect("list")
            .into_iter()
            .map(|d| d.domain)
            .collect()
    }

    #[tokio::test]
    async fn trailing_dot_and_case_are_folded() {
        let db = Database::open_in_memory().await.expect("db");
        let resolver = Arc::new(MockPtrResolver::with_names(vec![
            "Example.COM.".into(),
            "WWW.Example.COM".into(),
        ]));
        let worker = DnsWorker::spawn(db.clone(), resolver.clone(), fast_config(true));

        let ip = IpAddr::from_str("203.0.113.50").unwrap();
        assert!(worker.handle().enqueue_and_wait(ip).await);
        assert_eq!(resolver.call_count(), 1);

        let domains = domains_for(&db, "203.0.113.50").await;
        assert_eq!(domains, vec!["example.com", "www.example.com"]);

        let ip_id = upsert_ip_address(db.pool(), "203.0.113.50", Utc::now())
            .await
            .unwrap();
        let links = list_ip_domains(db.pool(), ip_id).await.unwrap();
        assert!(links.iter().all(|l| l.source == DomainSource::ReverseDns.as_str()));
        assert!(links
            .iter()
            .all(|l| (l.confidence.unwrap() - REVERSE_DNS_CONFIDENCE).abs() < f64::EPSILON));

        worker.shutdown().await;
    }

    #[tokio::test]
    async fn failed_lookup_stores_no_fake_domain() {
        let db = Database::open_in_memory().await.expect("db");
        let resolver = Arc::new(MockPtrResolver::failing("nxdomain"));
        let worker = DnsWorker::spawn(db.clone(), resolver.clone(), fast_config(true));

        let ip = IpAddr::from_str("198.51.100.9").unwrap();
        assert!(worker.handle().enqueue_and_wait(ip).await);
        assert_eq!(resolver.call_count(), 1);

        let domains = domains_for(&db, "198.51.100.9").await;
        assert!(domains.is_empty(), "failed PTR must not invent a domain");

        // Negative cache: second sighting must not call the resolver again.
        assert!(worker.handle().enqueue_and_wait(ip).await);
        assert_eq!(resolver.call_count(), 1);

        worker.shutdown().await;
    }

    #[tokio::test]
    async fn cache_hit_skips_the_resolver() {
        let db = Database::open_in_memory().await.expect("db");
        let resolver = Arc::new(MockPtrResolver::with_names(vec!["cdn.example.net.".into()]));
        let worker = DnsWorker::spawn(db.clone(), resolver.clone(), fast_config(true));
        let handle = worker.handle();

        let ip = IpAddr::from_str("203.0.113.77").unwrap();
        assert!(handle.enqueue_and_wait(ip).await);
        assert_eq!(resolver.call_count(), 1);
        assert_eq!(domains_for(&db, "203.0.113.77").await, vec!["cdn.example.net"]);

        assert!(handle.enqueue_and_wait(ip).await);
        assert_eq!(resolver.call_count(), 1, "cached IP must not hit resolver again");
        assert_eq!(domains_for(&db, "203.0.113.77").await, vec!["cdn.example.net"]);

        worker.shutdown().await;
    }

    #[tokio::test]
    async fn two_names_for_one_ip_both_remain() {
        let db = Database::open_in_memory().await.expect("db");
        let resolver = Arc::new(MockPtrResolver::with_names(vec![
            "alpha.example.org.".into(),
            "bravo.example.org.".into(),
        ]));
        let worker = DnsWorker::spawn(db.clone(), resolver.clone(), fast_config(true));

        let ip = IpAddr::from_str("192.0.2.44").unwrap();
        assert!(worker.handle().enqueue_and_wait(ip).await);

        let domains = domains_for(&db, "192.0.2.44").await;
        assert_eq!(domains, vec!["alpha.example.org", "bravo.example.org"]);

        worker.shutdown().await;
    }

    #[tokio::test]
    async fn pipeline_finishes_while_lookup_pending() {
        let db = Database::open_in_memory().await.expect("db");
        let gate = Arc::new(Notify::new());
        let resolver = Arc::new(MockPtrResolver::with_gate(
            vec!["slow.example.com.".into()],
            gate.clone(),
        ));
        let worker = DnsWorker::spawn(db.clone(), resolver.clone(), fast_config(true));
        let handle = worker.handle();

        let ip = IpAddr::from_str("203.0.113.99").unwrap();
        assert!(handle.enqueue(ip));

        let pipeline_finished = async {
            upsert_ip_address(db.pool(), "203.0.113.99", Utc::now())
                .await
                .expect("pipeline upsert");
            "pipeline-ok"
        };

        let result = timeout(Duration::from_millis(500), pipeline_finished)
            .await
            .expect("pipeline must not wait on reverse DNS");
        assert_eq!(result, "pipeline-ok");

        timeout(Duration::from_secs(2), async {
            while resolver.call_count() == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("PTR should start while pipeline already finished");

        assert!(
            domains_for(&db, "203.0.113.99").await.is_empty(),
            "reverse DNS must still be pending"
        );

        gate.notify_one();
        let wait = timeout(Duration::from_secs(2), async {
            loop {
                if !domains_for(&db, "203.0.113.99").await.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(wait.is_ok(), "reverse DNS should complete after gate release");

        worker.shutdown().await;
    }
}
