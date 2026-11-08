//! Asynchronous IP registration and network enrichment.

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "ip-enrichment";

/// Returns true for addresses that must not receive public registration lookups.
pub fn is_special_use_ip_str(ip: &str) -> bool {
    matches!(
        ip,
        "127.0.0.1"
            | "::1"
            | "0.0.0.0"
            | "::"
            | "255.255.255.255"
    ) || ip.starts_with("10.")
        || ip.starts_with("192.168.")
        || ip.starts_with("169.254.")
        || ip.starts_with("fc")
        || ip.starts_with("fd")
        || ip.starts_with("fe80:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_ip_enrichment() {
        assert_eq!(CRATE_NAME, "ip-enrichment");
    }

    #[test]
    fn special_use_ips_skip_rdap() {
        assert!(is_special_use_ip_str("127.0.0.1"));
        assert!(is_special_use_ip_str("192.168.1.1"));
        assert!(is_special_use_ip_str("10.0.0.1"));
        assert!(!is_special_use_ip_str("1.1.1.1"));
    }
}
