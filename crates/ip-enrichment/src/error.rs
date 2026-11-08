//! Enrichment and RDAP errors.

use std::fmt;

/// Failures while looking up or persisting IP enrichment.
#[derive(Debug)]
pub enum EnrichmentError {
    /// HTTP transport or status failure.
    Http(String),
    /// Response body could not be parsed as usable RDAP data.
    Parse(String),
    /// Lookup exceeded the configured timeout.
    Timeout,
    /// Persistence failed.
    Database(database::DatabaseError),
}

impl fmt::Display for EnrichmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(msg) => write!(f, "enrichment HTTP error: {msg}"),
            Self::Parse(msg) => write!(f, "enrichment parse error: {msg}"),
            Self::Timeout => write!(f, "enrichment lookup timed out"),
            Self::Database(err) => write!(f, "enrichment database error: {err}"),
        }
    }
}

impl std::error::Error for EnrichmentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(err) => Some(err),
            _ => None,
        }
    }
}

impl From<database::DatabaseError> for EnrichmentError {
    fn from(value: database::DatabaseError) -> Self {
        Self::Database(value)
    }
}

impl From<reqwest::Error> for EnrichmentError {
    fn from(value: reqwest::Error) -> Self {
        if value.is_timeout() {
            Self::Timeout
        } else {
            Self::Http(value.to_string())
        }
    }
}
