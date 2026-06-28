//! Birdeye token price source.
//!
//! The source treats the [`PriceSource`] `symbol` argument as a token address,
//! matching Birdeye's `address` query parameter.

use std::time::Duration;

use doppler_price_source::{scalar_to_minor, PriceSource};
use serde_json::Value;

/// Default Birdeye API host.
pub const DEFAULT_BASE_URL: &str = "https://public-api.birdeye.so";

/// Default Birdeye chain header.
pub const DEFAULT_CHAIN: &str = "solana";

/// Birdeye token price source.
pub struct Birdeye {
    client: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
    chain: String,
}

impl Birdeye {
    /// Build a Birdeye source using the required API key.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("doppler-feeder")
            .build()
            .expect("failed to build http client");
        Self {
            client,
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key: api_key.into(),
            chain: DEFAULT_CHAIN.to_string(),
        }
    }

    /// Override the API base, e.g. for tests or a proxy.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Override the `x-chain` header.
    #[must_use]
    pub fn with_chain(mut self, chain: impl Into<String>) -> Self {
        self.chain = chain.into();
        self
    }

    /// Fetch the latest price for `address`, returned as integer minor units.
    pub fn price(&self, address: &str, decimals: u32) -> Result<u64, String> {
        let url = format!("{}/defi/price", self.base_url);
        let body: Value = self
            .client
            .get(&url)
            .query(&[("address", address)])
            .header("X-API-KEY", &self.api_key)
            .header("x-chain", &self.chain)
            .send()
            .map_err(|e| format!("request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("http error: {e}"))?
            .json()
            .map_err(|e| format!("bad json: {e}"))?;
        price_from_response(&body, decimals)
    }
}

impl PriceSource for Birdeye {
    fn price_minor(&self, symbol: &str, decimals: u32) -> Result<u64, String> {
        self.price(symbol, decimals)
    }
}

/// Extract `data.value` from a Birdeye price response.
pub fn price_from_response(body: &Value, decimals: u32) -> Result<u64, String> {
    let price = body
        .get("data")
        .and_then(|data| data.get("value"))
        .ok_or_else(|| format!("missing data.value in response: {body}"))?;
    scalar_to_minor(price, decimals).ok_or_else(|| "unparseable data.value".to_string())
}

#[cfg(test)]
mod tests {
    use super::price_from_response;
    use serde_json::json;

    #[test]
    fn extracts_price() {
        let body = json!({
            "data": {
                "value": 152.123456789,
                "updateUnixTime": 1710000000,
                "updateHumanTime": "2024-03-09T16:00:00"
            },
            "success": true
        });
        assert_eq!(price_from_response(&body, 6), Ok(152_123_456));
    }

    #[test]
    fn missing_price_errors() {
        let body = json!({ "success": true, "data": {} });
        assert!(price_from_response(&body, 6).is_err());
    }

    #[test]
    fn invalid_price_errors() {
        let body = json!({ "success": true, "data": { "value": true } });
        assert!(price_from_response(&body, 6).is_err());
    }
}
