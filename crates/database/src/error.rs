//! Database error type.

use std::fmt;

/// Failures from opening, migrating, or querying SQLite.
#[derive(Debug)]
pub enum DatabaseError {
    Sqlx(sqlx::Error),
    Migrate(sqlx::migrate::MigrateError),
    Io(std::io::Error),
    /// Caller-facing validation or not-found message.
    Message(String),
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlx(err) => write!(f, "database error: {err}"),
            Self::Migrate(err) => write!(f, "migration error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Message(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DatabaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlx(err) => Some(err),
            Self::Migrate(err) => Some(err),
            Self::Io(err) => Some(err),
            Self::Message(_) => None,
        }
    }
}

impl From<sqlx::Error> for DatabaseError {
    fn from(value: sqlx::Error) -> Self {
        Self::Sqlx(value)
    }
}

impl From<sqlx::migrate::MigrateError> for DatabaseError {
    fn from(value: sqlx::migrate::MigrateError) -> Self {
        Self::Migrate(value)
    }
}

impl From<std::io::Error> for DatabaseError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
