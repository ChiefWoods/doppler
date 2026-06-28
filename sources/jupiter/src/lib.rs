//! Jupiter Price API source.
//!
//! The source treats the [`PriceSource`] `symbol` argument as a token mint
//! address, matching Jupiter's `ids` query parameter.

use std::time::Duration;

use doppler_price_source::{scalar_to_minor, PriceSource};
use serde_json::Value;

/// Default Jupiter API host.
pub const DEFAULT_BASE_URL: &str = "https://api.jup.ag";

/// Jupiter Price API source.
pub struct Jupiter {
    client: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
}

impl Jupiter {
    /// Build a Jupiter source using the required API key.
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
        }
    }

    /// Override the API base, e.g. for tests or a proxy.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Fetch the latest USD price for `mint`, returned as integer minor units.
    pub fn price(&self, mint: &str, decimals: u32) -> Result<u64, String> {
        let url = format!("{}/price/v3", self.base_url);
        let body: Value = self
            .client
            .get(&url)
            .query(&[("ids", mint)])
            .header("x-api-key", &self.api_key)
            .send()
            .map_err(|e| format!("request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("http error: {e}"))?
            .json()
            .map_err(|e| format!("bad json: {e}"))?;
        price_from_response(&body, mint, decimals)
    }
}

impl PriceSource for Jupiter {
    fn price_minor(&self, symbol: &str, decimals: u32) -> Result<u64, String> {
        self.price(symbol, decimals)
    }
}

/// Extract a mint's `usdPrice` from a Jupiter Price API response.
pub fn price_from_response(body: &Value, mint: &str, decimals: u32) -> Result<u64, String> {
    let price = body
        .get(mint)
        .and_then(|token| token.get("usdPrice"))
        .ok_or_else(|| format!("missing {mint}.usdPrice in response: {body}"))?;
    scalar_to_minor(price, decimals).ok_or_else(|| format!("unparseable usdPrice for {mint}"))
}

#[cfg(test)]
mod tests {
    use super::price_from_response;
    use serde_json::json;

    const SOL: &str = "So11111111111111111111111111111111111111112";

    #[test]
    fn extracts_price_for_mint() {
        let body = json!({
            SOL: {
                "usdPrice": 152.123456789,
                "blockId": 123,
                "decimals": 9,
                "priceChange24h": 1.5
            }
        });
        assert_eq!(price_from_response(&body, SOL, 6), Ok(152_123_456));
    }

    #[test]
    fn missing_mint_errors() {
        let body = json!({});
        assert!(price_from_response(&body, SOL, 6).is_err());
    }

    #[test]
    fn invalid_price_errors() {
        let body = json!({ SOL: { "usdPrice": true } });
        assert!(price_from_response(&body, SOL, 6).is_err());
    }
}
