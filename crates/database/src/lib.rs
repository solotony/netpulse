//! SQLite persistence, migrations, and historical queries.

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "database";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_database() {
        assert_eq!(CRATE_NAME, "database");
    }
}
