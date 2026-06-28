//! CoinGecko simple price source.
//!
//! The source treats the [`PriceSource`] `symbol` argument as a CoinGecko coin
//! id, matching the `ids` query parameter.

use std::time::Duration;

use doppler_price_source::{scalar_to_minor, PriceSource};
use serde_json::Value;

/// Default CoinGecko Demo API host.
pub const DEFAULT_BASE_URL: &str = "https://api.coingecko.com";

/// Default quote currency for simple price requests.
pub const DEFAULT_VS_CURRENCY: &str = "usd";

/// CoinGecko simple price source.
pub struct CoinGecko {
    client: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
    vs_currency: String,
}

impl CoinGecko {
    /// Build a CoinGecko source using the required Demo API key.
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
            vs_currency: DEFAULT_VS_CURRENCY.to_string(),
        }
    }

    /// Override the API base, e.g. for tests or a proxy.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Override the quote currency, e.g. `usd`, `eur`, or `btc`.
    #[must_use]
    pub fn with_vs_currency(mut self, vs_currency: impl Into<String>) -> Self {
        self.vs_currency = vs_currency.into();
        self
    }

    /// Fetch the latest price for `coin_id`, returned as integer minor units.
    pub fn price(&self, coin_id: &str, decimals: u32) -> Result<u64, String> {
        let url = format!("{}/api/v3/simple/price", self.base_url);
        let body: Value = self
            .client
            .get(&url)
            .query(&[
                ("ids", coin_id),
                ("vs_currencies", self.vs_currency.as_str()),
            ])
            .header("x-cg-demo-api-key", &self.api_key)
            .send()
            .map_err(|e| format!("request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("http error: {e}"))?
            .json()
            .map_err(|e| format!("bad json: {e}"))?;
        price_from_response(&body, coin_id, &self.vs_currency, decimals)
    }
}

impl PriceSource for CoinGecko {
    fn price_minor(&self, symbol: &str, decimals: u32) -> Result<u64, String> {
        self.price(symbol, decimals)
    }
}

/// Extract `body[coin_id][vs_currency]` from a CoinGecko simple price response.
pub fn price_from_response(
    body: &Value,
    coin_id: &str,
    vs_currency: &str,
    decimals: u32,
) -> Result<u64, String> {
    let price = body
        .get(coin_id)
        .and_then(|coin| coin.get(vs_currency))
        .ok_or_else(|| format!("missing {coin_id}.{vs_currency} in response: {body}"))?;
    scalar_to_minor(price, decimals).ok_or_else(|| format!("unparseable {coin_id}.{vs_currency}"))
}

#[cfg(test)]
mod tests {
    use super::price_from_response;
    use serde_json::json;

    #[test]
    fn extracts_price() {
        let body = json!({
            "bitcoin": {
                "usd": 67342.123456789
            }
        });
        assert_eq!(
            price_from_response(&body, "bitcoin", "usd", 6),
            Ok(67_342_123_456)
        );
    }

    #[test]
    fn missing_price_errors() {
        let body = json!({ "bitcoin": {} });
        assert!(price_from_response(&body, "bitcoin", "usd", 6).is_err());
    }

    #[test]
    fn invalid_price_errors() {
        let body = json!({ "bitcoin": { "usd": true } });
        assert!(price_from_response(&body, "bitcoin", "usd", 6).is_err());
    }
}
