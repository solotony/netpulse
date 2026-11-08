//! Application lifecycle, collector management, and event coordination.

mod mock_collector;

pub use mock_collector::MockCollector;

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "core";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_core() {
        assert_eq!(CRATE_NAME, "core");
    }
}
