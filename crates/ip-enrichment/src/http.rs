//! Mockable HTTP layer used by the RDAP client.

use async_trait::async_trait;

use crate::EnrichmentError;

/// Minimal GET client for RDAP over HTTP(S).
///
/// Production uses [`ReqwestHttpClient`]; tests inject counters / fixtures.
#[async_trait]
pub trait HttpClient: Send + Sync {
    /// Performs an HTTP GET and returns the response body as text.
    async fn get_text(&self, url: &str) -> Result<String, EnrichmentError>;
}

/// `reqwest`-backed [`HttpClient`].
#[derive(Debug, Clone)]
pub struct ReqwestHttpClient {
    client: reqwest::Client,
}

impl ReqwestHttpClient {
    /// Builds a client with no default timeout (callers apply enrichment timeouts).
    pub fn new() -> Result<Self, EnrichmentError> {
        let client = reqwest::Client::builder()
            .user_agent("network-monitor-ip-enrichment/0.1")
            .build()
            .map_err(|err| EnrichmentError::Http(err.to_string()))?;
        Ok(Self { client })
    }
}

impl Default for ReqwestHttpClient {
    fn default() -> Self {
        Self::new().expect("reqwest client")
    }
}

#[async_trait]
impl HttpClient for ReqwestHttpClient {
    async fn get_text(&self, url: &str) -> Result<String, EnrichmentError> {
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            return Err(EnrichmentError::Http(format!(
                "unexpected status {} for {url}",
                response.status()
            )));
        }
        Ok(response.text().await?)
    }
}
