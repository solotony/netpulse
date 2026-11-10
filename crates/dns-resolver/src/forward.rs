//! Injectable forward DNS (A / AAAA) lookup trait and hickory-backed implementation.

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use hickory_resolver::TokioResolver;
use tokio::time::timeout;

use crate::DnsError;

/// Performs forward DNS lookups for A and AAAA records.
///
/// Production uses [`HickoryForwardResolver`]; tests inject fixtures.
#[async_trait]
pub trait ForwardResolver: Send + Sync {
    /// Returns distinct IP addresses from A and AAAA answers for `name`.
    ///
    /// An empty `Ok` vector means no address records were found.
    async fn lookup_addresses(&self, name: &str) -> Result<Vec<IpAddr>, DnsError>;
}

/// `hickory-resolver`-backed [`ForwardResolver`] with a per-lookup timeout.
#[derive(Debug)]
pub struct HickoryForwardResolver {
    resolver: TokioResolver,
    timeout: Duration,
}

impl HickoryForwardResolver {
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
impl ForwardResolver for HickoryForwardResolver {
    async fn lookup_addresses(&self, name: &str) -> Result<Vec<IpAddr>, DnsError> {
        let lookup = timeout(self.timeout, self.resolver.lookup_ip(name))
            .await
            .map_err(|_| DnsError::Timeout)?;

        match lookup {
            Ok(response) => {
                let addresses: BTreeSet<IpAddr> = response.iter().collect();
                Ok(addresses.into_iter().collect())
            }
            Err(err) if is_no_records(&err) => Ok(Vec::new()),
            Err(err) => Err(DnsError::Lookup(err.to_string())),
        }
    }
}

fn is_no_records(err: &impl std::fmt::Display) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("no records")
        || text.contains("nxdomain")
        || text.contains("no such name")
        || text.contains("not found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::sync::Mutex;

    struct FixtureForward {
        responses: Mutex<Vec<Result<Vec<IpAddr>, DnsError>>>,
    }

    #[async_trait]
    impl ForwardResolver for FixtureForward {
        async fn lookup_addresses(&self, _name: &str) -> Result<Vec<IpAddr>, DnsError> {
            let mut guard = self.responses.lock().unwrap();
            if guard.is_empty() {
                return Ok(Vec::new());
            }
            guard.remove(0)
        }
    }

    #[tokio::test]
    async fn fixture_returns_a_and_aaaa() {
        let resolver = FixtureForward {
            responses: Mutex::new(vec![Ok(vec![
                IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)),
                IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
            ])]),
        };
        let addrs = resolver.lookup_addresses("example.com").await.unwrap();
        assert_eq!(addrs.len(), 2);
        assert!(addrs.contains(&IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))));
        assert!(addrs.contains(&IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1))));
    }

    #[tokio::test]
    async fn fixture_empty_is_ok() {
        let resolver = FixtureForward {
            responses: Mutex::new(vec![Ok(Vec::new())]),
        };
        let addrs = resolver.lookup_addresses("missing.example").await.unwrap();
        assert!(addrs.is_empty());
    }
}
