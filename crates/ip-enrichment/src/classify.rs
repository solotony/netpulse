//! Special-use address classification for local enrichment.

use std::net::IpAddr;

/// Categories of addresses that must not receive public registration lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecialUseClass {
    /// RFC 1918 private IPv4 (10/8, 172.16/12, 192.168/16).
    Private,
    /// IPv4 or IPv6 loopback.
    Loopback,
    /// IPv4 169.254/16 or IPv6 fe80::/10.
    LinkLocal,
    /// IPv6 unique local address (fc00::/7).
    UniqueLocal,
    /// IPv4 or IPv6 multicast.
    Multicast,
    /// 0.0.0.0 or ::.
    Unspecified,
}

/// Result of classifying an IP for enrichment routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IpClass {
    /// Non-globally-routable / special-use; skip public registration lookup.
    SpecialUse(SpecialUseClass),
    /// Candidate for public registration lookup (e.g. RDAP).
    Public,
}

/// Classifies `ip` into special-use or public.
///
/// Precedence when multiple labels could apply: unspecified → loopback →
/// multicast → link-local → private / ULA → public.
pub fn classify(ip: IpAddr) -> IpClass {
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_unspecified() {
                IpClass::SpecialUse(SpecialUseClass::Unspecified)
            } else if v4.is_loopback() {
                IpClass::SpecialUse(SpecialUseClass::Loopback)
            } else if v4.is_multicast() {
                IpClass::SpecialUse(SpecialUseClass::Multicast)
            } else if v4.is_link_local() {
                IpClass::SpecialUse(SpecialUseClass::LinkLocal)
            } else if v4.is_private() {
                IpClass::SpecialUse(SpecialUseClass::Private)
            } else {
                IpClass::Public
            }
        }
        IpAddr::V6(v6) => {
            if v6.is_unspecified() {
                IpClass::SpecialUse(SpecialUseClass::Unspecified)
            } else if v6.is_loopback() {
                IpClass::SpecialUse(SpecialUseClass::Loopback)
            } else if v6.is_multicast() {
                IpClass::SpecialUse(SpecialUseClass::Multicast)
            } else if v6.is_unicast_link_local() {
                IpClass::SpecialUse(SpecialUseClass::LinkLocal)
            } else if is_unique_local(v6) {
                IpClass::SpecialUse(SpecialUseClass::UniqueLocal)
            } else {
                IpClass::Public
            }
        }
    }
}

/// Returns `true` when a public registration lookup (RDAP / WHOIS) is allowed.
///
/// Special-use addresses always return `false`.
pub fn requires_public_lookup(ip: IpAddr) -> bool {
    matches!(classify(ip), IpClass::Public)
}

fn is_unique_local(addr: std::net::Ipv6Addr) -> bool {
    // fc00::/7 — first 7 bits are 1111 110x
    (addr.octets()[0] & 0xfe) == 0xfc
}
