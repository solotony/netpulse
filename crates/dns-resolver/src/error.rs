//! Reverse DNS error type.

use std::fmt;

/// Failures while resolving or persisting reverse DNS results.
#[derive(Debug)]
pub enum DnsError {
    /// Underlying resolver / network failure.
    Lookup(String),
    /// Lookup exceeded the configured timeout.
    Timeout,
    /// Persistence failed.
    Database(database::DatabaseError),
}

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lookup(msg) => write!(f, "DNS lookup error: {msg}"),
            Self::Timeout => write!(f, "DNS lookup timed out"),
            Self::Database(err) => write!(f, "DNS database error: {err}"),
        }
    }
}

impl std::error::Error for DnsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(err) => Some(err),
            _ => None,
        }
    }
}

impl From<database::DatabaseError> for DnsError {
    fn from(value: database::DatabaseError) -> Self {
        Self::Database(value)
    }
}
