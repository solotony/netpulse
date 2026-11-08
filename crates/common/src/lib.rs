//! Shared types and utilities for Network Monitor.

mod domain;
mod event;
mod ip;
mod process;

pub use domain::{normalize_domain, DomainError};
pub use event::{ConnectionEvent, ConnectionEventType, Direction, Platform, Protocol};
pub use ip::{IpAddress, IpParseError};
pub use process::ProcessInstanceKey;

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
