//! Process identity resolution (PID together with process start time).

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "process-info";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_process_info() {
        assert_eq!(CRATE_NAME, "process-info");
    }
}
