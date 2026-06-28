//! Binance public ticker price source.

use std::time::Duration;

use doppler_price_source::{parse_decimal_to_minor, PriceSource};
use serde_json::Value;

/// Binance public ticker price (`/api/v3/ticker/price?symbol={BASE}{QUOTE}`). Keyless.
/// Verified shape: `{"symbol":"BTCUSDT","price":"60412.00000000"}`.
///
/// `api.binance.com` is geo-restricted in some regions (incl. the US): use
/// [`Binance::with_base_url`] with `https://data-api.binance.vision` (public market
/// data) or `https://api.binance.us` if needed. Quote defaults to `USDT` — Binance
/// has no USD spot pairs.
pub struct Binance {
    client: reqwest::blocking::Client,
    base_url: String,
    quote: String,
}

impl Binance {
    /// USDT-quoted Binance source with a 10s request timeout.
    #[must_use]
    pub fn usdt() -> Self {
        Self::new("USDT")
    }

    /// Build a Binance source quoted in `quote` (e.g. `"USDT"`, `"USDC"`).
    #[must_use]
    pub fn new(quote: &str) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("doppler-feeder")
            .build()
            .expect("failed to build http client");
        Self {
            client,
            base_url: "https://api.binance.com".to_string(),
            quote: quote.to_string(),
        }
    }

    /// Override the API base, e.g. `https://data-api.binance.vision` or `https://api.binance.us`.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

impl PriceSource for Binance {
    fn price_minor(&self, symbol: &str, decimals: u32) -> Result<u64, String> {
        let pair = format!("{}{}", symbol.to_uppercase(), self.quote);
        let url = format!("{}/api/v3/ticker/price?symbol={pair}", self.base_url);
        let body: Value = self
            .client
            .get(&url)
            .send()
            .map_err(|e| format!("request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("http error: {e}"))?
            .json()
            .map_err(|e| format!("bad json: {e}"))?;
        let price = body["price"]
            .as_str()
            .ok_or_else(|| format!("missing price in response: {body}"))?;
        parse_decimal_to_minor(price, decimals)
            .ok_or_else(|| format!("unparseable price {price:?}"))
    }
}
