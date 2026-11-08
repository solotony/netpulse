//! Asynchronous IP registration and network enrichment.
//!
//! Local classification and longest-prefix matching (Step 10) plus optional
//! asynchronous RDAP lookups (Step 11). Reverse DNS is Step 12.

mod classify;
mod error;
mod http;
mod match_prefix;
mod rdap;
mod worker;

pub use classify::{
    classify, requires_public_lookup, IpClass, SpecialUseClass,
};
pub use error::EnrichmentError;
pub use http::{HttpClient, ReqwestHttpClient};
pub use match_prefix::{
    longest_prefix_match, parse_cidr, CidrParseError, NetworkPrefix,
};
pub use rdap::{
    parse_rdap_ip_network, RdapClient, RdapClientConfig, RdapNetworkInfo, DEFAULT_RDAP_URL_TEMPLATE,
};
pub use worker::{EnrichmentConfig, EnrichmentHandle, EnrichmentWorker};

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "ip-enrichment";

#[cfg(test)]
mod tests {
    use super::*;
    use ipnet::IpNet;
    use std::net::IpAddr;
    use std::str::FromStr;

    #[test]
    fn crate_name_is_ip_enrichment() {
        assert_eq!(CRATE_NAME, "ip-enrichment");
    }

    #[test]
    fn special_use_addresses_skip_public_lookup() {
        let cases: &[(&str, SpecialUseClass)] = &[
            // Private IPv4 (RFC 1918)
            ("10.0.0.1", SpecialUseClass::Private),
            ("172.16.5.1", SpecialUseClass::Private),
            ("192.168.1.1", SpecialUseClass::Private),
            // Loopback
            ("127.0.0.1", SpecialUseClass::Loopback),
            ("::1", SpecialUseClass::Loopback),
            // Link-local
            ("169.254.10.20", SpecialUseClass::LinkLocal),
            ("fe80::1", SpecialUseClass::LinkLocal),
            // IPv6 ULA
            ("fc00::1", SpecialUseClass::UniqueLocal),
            ("fd12:3456:789a::1", SpecialUseClass::UniqueLocal),
            // Multicast
            ("224.0.0.1", SpecialUseClass::Multicast),
            ("ff02::1", SpecialUseClass::Multicast),
            // Unspecified
            ("0.0.0.0", SpecialUseClass::Unspecified),
            ("::", SpecialUseClass::Unspecified),
        ];

        for &(ip, expected) in cases {
            let addr = IpAddr::from_str(ip).unwrap_or_else(|_| panic!("parse {ip}"));
            assert_eq!(
                classify(addr),
                IpClass::SpecialUse(expected),
                "classify({ip})"
            );
            assert!(
                !requires_public_lookup(addr),
                "{ip} must skip public lookup"
            );
        }
    }

    #[test]
    fn globally_routable_addresses_require_public_lookup() {
        for ip in ["1.1.1.1", "8.8.8.8", "2001:4860:4860::8888", "2606:4700:4700::1111"] {
            let addr = IpAddr::from_str(ip).unwrap_or_else(|_| panic!("parse {ip}"));
            assert_eq!(classify(addr), IpClass::Public, "classify({ip})");
            assert!(requires_public_lookup(addr), "{ip} should allow public lookup");
        }
    }

    #[test]
    fn overlapping_prefixes_pick_longest() {
        let prefixes = [
            NetworkPrefix::parse("10.0.0.0/8").expect("cidr"),
            NetworkPrefix::parse("10.1.0.0/16").expect("cidr"),
            NetworkPrefix::parse("10.1.2.0/24").expect("cidr"),
            NetworkPrefix::parse("192.168.0.0/16").expect("cidr"),
        ];

        let ip = IpAddr::from_str("10.1.2.3").expect("ip");
        let winner = longest_prefix_match(ip, &prefixes).expect("match");
        assert_eq!(winner.cidr(), &IpNet::from_str("10.1.2.0/24").unwrap());
        assert_eq!(winner.prefix_len(), 24);
    }

    #[test]
    fn ipv6_longest_prefix_match() {
        let prefixes = [
            NetworkPrefix::parse("2001:db8::/32").expect("cidr"),
            NetworkPrefix::parse("2001:db8:abcd::/48").expect("cidr"),
            NetworkPrefix::parse("2001:db8:abcd:1234::/64").expect("cidr"),
            NetworkPrefix::parse("fc00::/7").expect("cidr"),
        ];

        let ip = IpAddr::from_str("2001:db8:abcd:1234::5").expect("ip");
        let winner = longest_prefix_match(ip, &prefixes).expect("match");
        assert_eq!(
            winner.cidr(),
            &IpNet::from_str("2001:db8:abcd:1234::/64").unwrap()
        );

        // No match when outside all prefixes.
        let other = IpAddr::from_str("2001:db9::1").expect("ip");
        assert!(longest_prefix_match(other, &prefixes).is_none());
    }

    #[test]
    fn one_ip_maps_to_single_most_specific_prefix() {
        let prefixes = [
            NetworkPrefix::parse("203.0.113.0/24").expect("cidr"),
            NetworkPrefix::parse("203.0.113.0/28").expect("cidr"),
        ];
        let ip = IpAddr::from_str("203.0.113.10").expect("ip");
        let matches: Vec<_> = prefixes
            .iter()
            .filter(|p| p.contains(ip))
            .collect();
        assert_eq!(matches.len(), 2, "setup: overlapping candidates");
        let winner = longest_prefix_match(ip, &prefixes).expect("winner");
        assert_eq!(winner.prefix_len(), 28);
    }
}
