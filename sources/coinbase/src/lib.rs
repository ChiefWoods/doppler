//! Coinbase public spot price source.

use std::time::Duration;

use doppler_price_source::{parse_decimal_to_minor, PriceSource};

/// Coinbase public spot price (`/v2/prices/{BASE}-{QUOTE}/spot`). Keyless and
/// US-reachable. Maps `"SOL"` -> `SOL-USD`, `"BTC"` -> `BTC-USD`, etc.
pub struct Coinbase {
    client: reqwest::blocking::Client,
    quote: String,
}

impl Coinbase {
    /// USD-quoted Coinbase source with a 10s request timeout.
    #[must_use]
    pub fn usd() -> Self {
        Self::new("USD")
    }

    /// Build a Coinbase source quoted in `quote` (e.g. `"USD"`).
    #[must_use]
    pub fn new(quote: &str) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("doppler-feeder")
            .build()
            .expect("failed to build http client");
        Self {
            client,
            quote: quote.to_string(),
        }
    }
}

impl PriceSource for Coinbase {
    fn price_minor(&self, symbol: &str, decimals: u32) -> Result<u64, String> {
        let url = format!(
            "https://api.coinbase.com/v2/prices/{symbol}-{}/spot",
            self.quote
        );
        let body: serde_json::Value = self
            .client
            .get(&url)
            .send()
            .map_err(|e| format!("request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("http error: {e}"))?
            .json()
            .map_err(|e| format!("bad json: {e}"))?;

        let amount = body["data"]["amount"]
            .as_str()
            .ok_or_else(|| format!("missing data.amount in response: {body}"))?;

        parse_decimal_to_minor(amount, decimals)
            .ok_or_else(|| format!("unparseable amount {amount:?}"))
    }
}
