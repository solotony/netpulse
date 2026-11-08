//! Domain name normalization helpers.

use std::fmt;

/// Error returned when a domain name cannot be normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    Empty,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "domain name is empty"),
        }
    }
}

impl std::error::Error for DomainError {}

/// Normalizes a domain name for storage and comparison.
///
/// - Trims surrounding whitespace
/// - Lowercases ASCII letters
/// - Strips a single trailing dot (FQDN form)
/// - Rejects empty results (including `"."` and whitespace-only input)
pub fn normalize_domain(input: &str) -> Result<String, DomainError> {
    let trimmed = input.trim();
    let without_trailing_dot = trimmed.strip_suffix('.').unwrap_or(trimmed);
    let normalized = without_trailing_dot.to_ascii_lowercase();

    if normalized.is_empty() {
        return Err(DomainError::Empty);
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_domain() {
        assert_eq!(normalize_domain("Example.COM").unwrap(), "example.com");
    }

    #[test]
    fn strips_trailing_dot() {
        assert_eq!(normalize_domain("example.com.").unwrap(), "example.com");
    }

    #[test]
    fn rejects_empty_domain() {
        assert_eq!(normalize_domain(""), Err(DomainError::Empty));
        assert_eq!(normalize_domain("   "), Err(DomainError::Empty));
        assert_eq!(normalize_domain("."), Err(DomainError::Empty));
        assert_eq!(normalize_domain(" . "), Err(DomainError::Empty));
    }
}
