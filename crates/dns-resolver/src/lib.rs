//! Domain resolution and reverse DNS for remote IP addresses.
//!
//! MVP domain discovery is **reverse DNS (PTR) only** via an injectable resolver
//! trait (`hickory-resolver` in production, mocks in tests). Observed DNS / OS
//! cache correlation is intentionally unavailable.

mod error;
mod ptr;
mod worker;

pub use error::DnsError;
pub use ptr::{HickoryPtrResolver, PtrResolver};
pub use worker::{DnsConfig, DnsHandle, DnsWorker};

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "dns-resolver";

/// Confidence assigned to reverse-DNS associations.
///
/// PTR names identify the reverse zone owner, not necessarily the hostname a
/// user visited, so reliability is moderate.
pub const REVERSE_DNS_CONFIDENCE: f64 = 0.5;

/// MVP domain discovery sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainSource {
    /// Reverse DNS (PTR) lookup — only source enabled for the DNS MVP.
    ReverseDns,
}

impl DomainSource {
    /// Stable string persisted in `ip_domains.source`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReverseDns => "REVERSE_DNS",
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
    fn mvp_source_is_reverse_dns() {
        assert_eq!(DomainSource::ReverseDns.as_str(), "REVERSE_DNS");
    }

    #[test]
    fn normalize_folds_trailing_dot_and_case() {
        assert_eq!(
            normalize_domain("Example.COM.").unwrap(),
            "example.com"
        );
    }
}
