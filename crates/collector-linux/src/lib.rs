//! Linux network connection collector (not yet implemented).

use collector_api::{CollectorCapabilities, Platform};

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "collector-linux";

/// Error returned by unimplemented collector operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorError {
    message: &'static str,
}

impl CollectorError {
    pub fn not_implemented() -> Self {
        Self {
            message: "Linux collector is not implemented",
        }
    }

    pub fn message(&self) -> &'static str {
        self.message
    }
}

/// Linux collector stub — start returns not-implemented; capabilities are all false.
#[derive(Debug, Default)]
pub struct LinuxCollector;

impl LinuxCollector {
    pub fn platform(&self) -> Platform {
        Platform::Linux
    }

    pub fn capabilities(&self) -> CollectorCapabilities {
        CollectorCapabilities::none()
    }

    pub fn start(&mut self) -> Result<(), CollectorError> {
        Err(CollectorError::not_implemented())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_collector_linux() {
        assert_eq!(CRATE_NAME, "collector-linux");
    }

    #[test]
    fn platform_stub_identity() {
        let mut collector = LinuxCollector;
        assert_eq!(collector.platform(), Platform::Linux);
        assert_eq!(collector.capabilities(), CollectorCapabilities::none());
        assert_eq!(
            collector.start().unwrap_err().message(),
            "Linux collector is not implemented"
        );
    }
}
