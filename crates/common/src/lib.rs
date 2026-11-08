//! Shared types and utilities for Network Monitor.

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "common";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_common() {
        assert_eq!(CRATE_NAME, "common");
    }
}
