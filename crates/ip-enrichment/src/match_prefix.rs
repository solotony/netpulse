//! Longest-prefix matching against known network CIDRs.

use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;

use ipnet::IpNet;

/// A known network prefix eligible for longest-prefix matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkPrefix {
    cidr: IpNet,
}

impl NetworkPrefix {
    /// Builds a prefix from a parsed [`IpNet`].
    pub fn new(cidr: IpNet) -> Self {
        Self { cidr }
    }

    /// Parses a CIDR string such as `10.0.0.0/8` or `2001:db8::/32`.
    pub fn parse(cidr: &str) -> Result<Self, CidrParseError> {
        parse_cidr(cidr).map(Self::new)
    }

    /// The underlying CIDR.
    pub fn cidr(&self) -> &IpNet {
        &self.cidr
    }

    /// Prefix length in bits.
    pub fn prefix_len(&self) -> u8 {
        self.cidr.prefix_len()
    }

    /// Whether `ip` falls inside this prefix.
    pub fn contains(&self, ip: IpAddr) -> bool {
        self.cidr.contains(&ip)
    }
}

impl From<IpNet> for NetworkPrefix {
    fn from(cidr: IpNet) -> Self {
        Self::new(cidr)
    }
}

impl fmt::Display for NetworkPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cidr.fmt(f)
    }
}

/// Error returned when a CIDR string cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidrParseError {
    pub value: String,
}

impl fmt::Display for CidrParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid CIDR: {}", self.value)
    }
}

impl std::error::Error for CidrParseError {}

/// Parses a CIDR notation string into an [`IpNet`].
pub fn parse_cidr(cidr: &str) -> Result<IpNet, CidrParseError> {
    IpNet::from_str(cidr).map_err(|_| CidrParseError {
        value: cidr.to_owned(),
    })
}

/// Selects the most specific (longest) prefix that contains `ip`.
///
/// When multiple prefixes match, only the longest prefix length wins. Ties of
/// equal length return one of the tied prefixes (stable with respect to
/// iterator order via `max_by_key`).
///
/// Returns `None` when no prefix contains `ip`.
pub fn longest_prefix_match<'a, I>(ip: IpAddr, prefixes: I) -> Option<&'a NetworkPrefix>
where
    I: IntoIterator<Item = &'a NetworkPrefix>,
{
    prefixes
        .into_iter()
        .filter(|prefix| prefix.contains(ip))
        .max_by_key(|prefix| prefix.prefix_len())
}
