//! IP address newtype with parse helpers.

use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Wrapper around [`IpAddr`] used in the shared event model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IpAddress(IpAddr);

impl IpAddress {
    pub fn new(addr: IpAddr) -> Self {
        Self(addr)
    }

    pub fn inner(self) -> IpAddr {
        self.0
    }

    pub fn as_ip_addr(&self) -> &IpAddr {
        &self.0
    }

    pub fn is_ipv4(&self) -> bool {
        self.0.is_ipv4()
    }

    pub fn is_ipv6(&self) -> bool {
        self.0.is_ipv6()
    }

    /// Parses an IPv4 or IPv6 address string.
    pub fn parse(s: &str) -> Result<Self, IpParseError> {
        s.parse()
    }
}

impl From<IpAddr> for IpAddress {
    fn from(addr: IpAddr) -> Self {
        Self(addr)
    }
}

impl From<IpAddress> for IpAddr {
    fn from(addr: IpAddress) -> Self {
        addr.0
    }
}

impl FromStr for IpAddress {
    type Err = IpParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        IpAddr::from_str(s)
            .map(Self)
            .map_err(|_| IpParseError::InvalidAddress(s.to_owned()))
    }
}

impl fmt::Display for IpAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Error returned when an IP address string cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpParseError {
    InvalidAddress(String),
}

impl fmt::Display for IpParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAddress(value) => write!(f, "invalid IP address: {value}"),
        }
    }
}

impl std::error::Error for IpParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn parses_ipv4() {
        let addr = IpAddress::parse("192.0.2.10").expect("valid IPv4");
        assert!(addr.is_ipv4());
        assert_eq!(addr.inner(), IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)));
    }

    #[test]
    fn parses_ipv6() {
        let addr = IpAddress::parse("2001:db8::1").expect("valid IPv6");
        assert!(addr.is_ipv6());
        assert_eq!(
            addr.inner(),
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1))
        );
    }

    #[test]
    fn rejects_invalid_address() {
        assert!(IpAddress::parse("not-an-ip").is_err());
    }
}
