//! Injectable PTR lookup trait and hickory-backed implementation.

use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use hickory_resolver::proto::rr::RData;
use hickory_resolver::TokioResolver;
use tokio::time::timeout;

use crate::DnsError;

/// Performs reverse DNS (PTR) lookups.
///
/// Production uses [`HickoryPtrResolver`]; tests inject fixtures / call counters.
#[async_trait]
pub trait PtrResolver: Send + Sync {
    /// Returns raw PTR owner names for `ip` (may include trailing dots / mixed case).
    ///
    /// An empty `Ok` vector means no PTR records were found — callers must not invent a domain.
    async fn lookup_ptr(&self, ip: IpAddr) -> Result<Vec<String>, DnsError>;
}

/// `hickory-resolver`-backed [`PtrResolver`] with a per-lookup timeout.
#[derive(Debug)]
pub struct HickoryPtrResolver {
    resolver: TokioResolver,
    timeout: Duration,
}

impl HickoryPtrResolver {
    /// Builds a system-configured Tokio resolver.
    pub fn new(timeout: Duration) -> Result<Self, DnsError> {
        let resolver = TokioResolver::builder_tokio()
            .map_err(|err| DnsError::Lookup(err.to_string()))?
            .build()
            .map_err(|err| DnsError::Lookup(err.to_string()))?;
        Ok(Self { resolver, timeout })
    }

    /// Wraps an existing resolver (useful for custom name-server configs).
    pub fn with_resolver(resolver: TokioResolver, timeout: Duration) -> Self {
        Self { resolver, timeout }
    }
}

#[async_trait]
impl PtrResolver for HickoryPtrResolver {
    async fn lookup_ptr(&self, ip: IpAddr) -> Result<Vec<String>, DnsError> {
        let lookup = timeout(self.timeout, self.resolver.reverse_lookup(ip))
            .await
            .map_err(|_| DnsError::Timeout)?
            .map_err(|err| DnsError::Lookup(err.to_string()))?;

        Ok(lookup
            .answers()
            .iter()
            .filter_map(|record| match &record.data {
                RData::PTR(name) => Some(name.to_ascii()),
                _ => None,
            })
            .collect())
    }
}
