//! Windows network connection collector (not yet implemented).

use collector_api::{CollectorCapabilities, Platform};

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "collector-windows";

/// Error returned by unimplemented collector operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorError {
    message: &'static str,
}

impl CollectorError {
    pub fn not_implemented() -> Self {
        Self {
            message: "Windows collector is not implemented",
        }
    }

    pub fn message(&self) -> &'static str {
        self.message
    }
}

/// Windows collector stub — start returns not-implemented; capabilities are all false.
#[derive(Debug, Default)]
pub struct WindowsCollector;

impl WindowsCollector {
    pub fn platform(&self) -> Platform {
        Platform::Windows
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
    fn crate_name_is_collector_windows() {
        assert_eq!(CRATE_NAME, "collector-windows");
    }

    #[test]
    fn platform_stub_identity() {
        let mut collector = WindowsCollector;
        assert_eq!(collector.platform(), Platform::Windows);
        assert_eq!(collector.capabilities(), CollectorCapabilities::none());
        assert_eq!(
            collector.start().unwrap_err().message(),
            "Windows collector is not implemented"
        );
    }
}
