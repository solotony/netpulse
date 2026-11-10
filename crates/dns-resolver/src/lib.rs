//! Domain resolution: reverse DNS (PTR) and forward DNS (A / AAAA).
//!
//! Background discovery remains PTR-based. Manual domain entry uses forward
//! lookups via [`ForwardResolver`]. Observed DNS / OS cache correlation is
//! intentionally unavailable.

mod error;
mod forward;
mod ptr;
mod worker;

pub use error::DnsError;
pub use forward::{ForwardResolver, HickoryForwardResolver};
pub use ptr::{HickoryPtrResolver, PtrResolver};
pub use worker::{DnsConfig, DnsHandle, DnsWorker};

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "dns-resolver";

/// Confidence assigned to reverse-DNS associations.
///
/// PTR names identify the reverse zone owner, not necessarily the hostname a
/// user visited, so reliability is moderate.
pub const REVERSE_DNS_CONFIDENCE: f64 = 0.5;

/// Confidence assigned to forward-DNS associations from manual domain entry.
pub const FORWARD_DNS_CONFIDENCE: f64 = 0.95;

/// Domain discovery sources persisted in `ip_domains.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainSource {
    /// Reverse DNS (PTR) lookup.
    ReverseDns,
    /// Forward DNS (A / AAAA) from manual domain entry.
    ForwardDns,
}

impl DomainSource {
    /// Stable string persisted in `ip_domains.source`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReverseDns => "REVERSE_DNS",
            Self::ForwardDns => "FORWARD_DNS",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::normalize_domain;

    #[test]
    fn crate_name_is_dns_resolver() {
        assert_eq!(CRATE_NAME, "dns-resolver");
    }

    #[test]
    fn source_labels() {
        assert_eq!(DomainSource::ReverseDns.as_str(), "REVERSE_DNS");
        assert_eq!(DomainSource::ForwardDns.as_str(), "FORWARD_DNS");
    }

    #[test]
    fn normalize_folds_trailing_dot_and_case() {
        assert_eq!(
            normalize_domain("Example.COM.").unwrap(),
            "example.com"
        );
    }
}
