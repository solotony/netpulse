//! Historical aggregation by process, IP, domain, network, and time.

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "statistics";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_statistics() {
        assert_eq!(CRATE_NAME, "statistics");
    }
}
