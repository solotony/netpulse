//! Domain resolution and reverse DNS for remote IP addresses.

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "dns-resolver";

/// MVP domain discovery sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainSource {
    /// Reverse DNS (PTR) lookup — only source enabled for the DNS MVP.
    ReverseDns,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_dns_resolver() {
        assert_eq!(CRATE_NAME, "dns-resolver");
    }

    #[test]
    fn mvp_source_is_reverse_dns() {
        assert_eq!(DomainSource::ReverseDns, DomainSource::ReverseDns);
    }
}
