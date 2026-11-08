//! RDAP IP network lookup and response parsing.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use database::NewNetwork;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use crate::http::HttpClient;
use crate::EnrichmentError;

/// Default RDAP bootstrap-style IP query template (`{ip}` substituted).
pub const DEFAULT_RDAP_URL_TEMPLATE: &str = "https://rdap.org/ip/{ip}";

/// Tunables for RDAP HTTP behavior.
#[derive(Debug, Clone)]
pub struct RdapClientConfig {
    /// URL template containing `{ip}`.
    pub url_template: String,
    /// Per-attempt HTTP timeout.
    pub timeout: Duration,
    /// Extra attempts after the first failure.
    pub max_retries: u32,
    /// Delay between retries.
    pub retry_delay: Duration,
    /// Minimum spacing between successful HTTP starts (rate limit).
    pub min_interval: Duration,
}

impl Default for RdapClientConfig {
    fn default() -> Self {
        Self {
            url_template: DEFAULT_RDAP_URL_TEMPLATE.to_owned(),
            timeout: Duration::from_secs(5),
            max_retries: 2,
            retry_delay: Duration::from_millis(200),
            min_interval: Duration::from_millis(200),
        }
    }
}

/// Parsed registration fields from an RDAP IP network object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdapNetworkInfo {
    pub cidr: String,
    pub net_range: Option<String>,
    pub net_name: Option<String>,
    pub organization: Option<String>,
    pub asn: Option<i64>,
    pub country: Option<String>,
}

impl RdapNetworkInfo {
    /// Converts into a [`NewNetwork`] row with `source = "rdap"`.
    pub fn into_new_network(self, updated_at: DateTime<Utc>) -> NewNetwork {
        NewNetwork {
            cidr: self.cidr,
            net_range: self.net_range,
            net_name: self.net_name,
            organization: self.organization,
            asn: self.asn,
            country: self.country,
            source: Some("rdap".into()),
            updated_at,
        }
    }
}

/// RDAP client with timeout, retry, and simple rate limiting.
pub struct RdapClient<C: HttpClient> {
    http: Arc<C>,
    config: RdapClientConfig,
    last_request_at: Mutex<Option<tokio::time::Instant>>,
}

impl<C: HttpClient> RdapClient<C> {
    /// Wraps an injectable HTTP client.
    pub fn new(http: Arc<C>, config: RdapClientConfig) -> Self {
        Self {
            http,
            config,
            last_request_at: Mutex::new(None),
        }
    }

    /// Looks up registration data for `ip` (caller must gate special-use / settings).
    pub async fn lookup_ip(&self, ip: IpAddr) -> Result<RdapNetworkInfo, EnrichmentError> {
        let url = self
            .config
            .url_template
            .replace("{ip}", &ip.to_string());

        let attempts = self.config.max_retries + 1;
        let mut last_err = EnrichmentError::Http("RDAP lookup failed".into());

        for attempt in 0..attempts {
            if attempt > 0 {
                sleep(self.config.retry_delay).await;
            }

            self.await_rate_limit().await;

            match self.attempt_once(&url).await {
                Ok(info) => return Ok(info),
                Err(err) => last_err = err,
            }
        }

        Err(last_err)
    }

    async fn await_rate_limit(&self) {
        let mut guard = self.last_request_at.lock().await;
        if let Some(previous) = *guard {
            let elapsed = previous.elapsed();
            if elapsed < self.config.min_interval {
                sleep(self.config.min_interval - elapsed).await;
            }
        }
        *guard = Some(tokio::time::Instant::now());
    }

    async fn attempt_once(&self, url: &str) -> Result<RdapNetworkInfo, EnrichmentError> {
        let body = match timeout(self.config.timeout, self.http.get_text(url)).await {
            Ok(Ok(body)) => body,
            Ok(Err(err)) => return Err(err),
            Err(_) => return Err(EnrichmentError::Timeout),
        };
        parse_rdap_ip_network(&body)
    }
}

/// Parses an RDAP IP network JSON document into enrichment fields.
///
/// Missing country / ASN stay `None` — never invented.
pub fn parse_rdap_ip_network(body: &str) -> Result<RdapNetworkInfo, EnrichmentError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|err| EnrichmentError::Parse(err.to_string()))?;

    let cidr = extract_cidr(&value).ok_or_else(|| {
        EnrichmentError::Parse("RDAP response missing usable CIDR".into())
    })?;

    let start = value
        .get("startAddress")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let end = value
        .get("endAddress")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let net_range = match (start, end) {
        (Some(s), Some(e)) => Some(format!("{s} - {e}")),
        _ => None,
    };

    let net_name = value
        .get("name")
        .and_then(Value::as_str)
        .map(|s| s.to_owned());

    let country = value
        .get("country")
        .and_then(Value::as_str)
        .map(|s| s.to_owned());

    let organization = extract_organization(&value);
    let asn = extract_asn(&value);

    Ok(RdapNetworkInfo {
        cidr,
        net_range,
        net_name,
        organization,
        asn,
        country,
    })
}

fn extract_cidr(value: &Value) -> Option<String> {
    if let Some(cidrs) = value.get("cidr0_cidrs").and_then(Value::as_array) {
        for entry in cidrs {
            if let Some(prefix) = entry.get("v4prefix").and_then(Value::as_str) {
                let length = entry.get("length").and_then(Value::as_u64)?;
                return Some(format!("{prefix}/{length}"));
            }
            if let Some(prefix) = entry.get("v6prefix").and_then(Value::as_str) {
                let length = entry.get("length").and_then(Value::as_u64)?;
                return Some(format!("{prefix}/{length}"));
            }
        }
    }

    // Fallback: some servers expose a top-level "cidr" / "handle" style string.
    if let Some(cidr) = value.get("cidr").and_then(Value::as_str) {
        return Some(cidr.to_owned());
    }

    None
}

fn extract_organization(value: &Value) -> Option<String> {
    let entities = value.get("entities")?.as_array()?;
    for entity in entities {
        let roles = entity
            .get("roles")
            .and_then(Value::as_array)
            .map(|roles| {
                roles
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|r| r.eq_ignore_ascii_case("registrant"))
            })
            .unwrap_or(false);
        if !roles {
            continue;
        }
        if let Some(name) = vcard_fn(entity) {
            return Some(name);
        }
    }

    // Any entity with an FN if no explicit registrant.
    for entity in entities {
        if let Some(name) = vcard_fn(entity) {
            return Some(name);
        }
    }
    None
}

fn vcard_fn(entity: &Value) -> Option<String> {
    let vcard = entity.get("vcardArray")?.as_array()?;
    let entries = vcard.get(1)?.as_array()?;
    for entry in entries {
        let row = entry.as_array()?;
        if row.first().and_then(Value::as_str) == Some("fn") {
            return row.get(3).and_then(Value::as_str).map(str::to_owned);
        }
    }
    None
}

fn extract_asn(value: &Value) -> Option<i64> {
    // Prefer explicit enrichment-friendly field when present (tests / adapters).
    if let Some(asn) = value.get("asn").and_then(parse_asn_value) {
        return Some(asn);
    }

    let entities = value.get("entities")?.as_array()?;
    for entity in entities {
        if let Some(handle) = entity.get("handle").and_then(Value::as_str) {
            if let Some(asn) = parse_asn_str(handle) {
                return Some(asn);
            }
        }
        if let Some(asn) = entity.get("asn").and_then(parse_asn_value) {
            return Some(asn);
        }
    }
    None
}

fn parse_asn_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => parse_asn_str(s),
        _ => None,
    }
}

fn parse_asn_str(raw: &str) -> Option<i64> {
    let trimmed = raw.trim();
    let digits = trimmed
        .strip_prefix("AS")
        .or_else(|| trimmed.strip_prefix("as"))
        .unwrap_or(trimmed);
    digits.parse().ok()
}

/// Cloudflare-style RDAP fixture used by unit tests (spec §6.1).
#[cfg(test)]
pub fn cloudflare_rdap_fixture() -> String {
    serde_json::json!({
        "objectClassName": "ip network",
        "handle": "NET-172-64-0-0-1",
        "startAddress": "172.64.0.0",
        "endAddress": "172.71.255.255",
        "name": "CLOUDFLARENET",
        "country": "US",
        "cidr0_cidrs": [{ "v4prefix": "172.64.0.0", "length": 13 }],
        "entities": [{
            "objectClassName": "entity",
            "handle": "CLOUD14",
            "roles": ["registrant"],
            "vcardArray": ["vcard", [["fn", {}, "text", "Cloudflare, Inc."]]],
            "asn": "AS13335"
        }]
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cloudflare_style_rdap() {
        let info = parse_rdap_ip_network(&cloudflare_rdap_fixture()).expect("parse");
        assert_eq!(info.cidr, "172.64.0.0/13");
        assert_eq!(
            info.net_range.as_deref(),
            Some("172.64.0.0 - 172.71.255.255")
        );
        assert_eq!(info.net_name.as_deref(), Some("CLOUDFLARENET"));
        assert_eq!(info.organization.as_deref(), Some("Cloudflare, Inc."));
        assert_eq!(info.asn, Some(13335));
        assert_eq!(info.country.as_deref(), Some("US"));
    }

    #[test]
    fn missing_country_and_asn_stay_none() {
        let body = r#"{
            "name": "PARTIAL",
            "cidr0_cidrs": [{ "v4prefix": "203.0.113.0", "length": 24 }]
        }"#;
        let info = parse_rdap_ip_network(body).expect("parse");
        assert_eq!(info.cidr, "203.0.113.0/24");
        assert_eq!(info.net_name.as_deref(), Some("PARTIAL"));
        assert!(info.country.is_none());
        assert!(info.asn.is_none());
        assert!(info.organization.is_none());
    }
}
