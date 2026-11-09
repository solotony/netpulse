//! Application lifecycle, collector management, and event coordination.

mod mock_collector;
mod monitoring;
mod pipeline;

pub use mock_collector::MockCollector;
pub use monitoring::{
    MonitoringController, MonitoringError, MonitoringState, MonitoringStatus,
};
pub use pipeline::{EventPipeline, PipelineError, ProcessOutcome};

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "core";

#[cfg(test)]
mod pipeline_integration;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_core() {
        assert_eq!(CRATE_NAME, "core");
    }
}
