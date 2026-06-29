//! OKX OnchainOS v6 market price source.
//!
//! [`Okx`] calls `POST /api/v6/dex/market/price` with signed headers per
//! [OKX authentication](https://web3.okx.com/onchainos/dev-docs/home/api-access-and-usage).
//! The [`PriceSource`] `symbol` argument is the token contract address; set
//! [`Okx::chain_index`] for the chain (e.g. `"501"` for Solana, `"1"` for Ethereum).
//!
//! ```no_run
//! use doppler_price_source::PriceSource;
//! use doppler_price_source_okx::Okx;
//!
//! let src = Okx::new("API_KEY", "SECRET", "PASSPHRASE").chain_index("501");
//! let price = src.price_minor("JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN", 6).unwrap();
//! ```

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use doppler_price_source::{parse_decimal_to_minor, PriceSource};
use hmac::{Hmac, Mac};
use serde::Serialize;
use serde_json::Value;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Default OKX OnchainOS API host.
pub const DEFAULT_BASE_URL: &str = "https://web3.okx.com";

/// Market price endpoint path (v6).
pub const PRICE_PATH: &str = "/api/v6/dex/market/price";

/// OKX OnchainOS v6 signed market price source.
pub struct Okx {
    client: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
    secret_key: String,
    passphrase: String,
    chain_index: String,
}

#[derive(Serialize)]
struct PriceRequest<'a> {
    #[serde(rename = "chainIndex")]
    chain_index: &'a str,
    #[serde(rename = "tokenContractAddress")]
    token_contract_address: &'a str,
}

impl Okx {
    /// Build an OKX source. Call [`chain_index`](Self::chain_index) before fetching.
    #[must_use]
    pub fn new(
        api_key: impl Into<String>,
        secret_key: impl Into<String>,
        passphrase: impl Into<String>,
    ) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("doppler-feeder")
            .build()
            .expect("failed to build http client");
        Self {
            client,
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key: api_key.into(),
            secret_key: secret_key.into(),
            passphrase: passphrase.into(),
            chain_index: String::new(),
        }
    }

    /// Override the API base, e.g. for tests or a proxy.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Chain id for all requests, e.g. `"501"` (Solana) or `"1"` (Ethereum).
    #[must_use]
    pub fn chain_index(mut self, chain_index: impl Into<String>) -> Self {
        self.chain_index = chain_index.into();
        self
    }

    /// Fetch the latest USD price for `token_address`, returned as minor units.
    pub fn price(&self, token_address: &str, decimals: u32) -> Result<u64, String> {
        if self.chain_index.is_empty() {
            return Err("chain_index is not set".to_string());
        }
        let token_address = normalize_token_address(token_address);
        let body = serde_json::to_string(&[PriceRequest {
            chain_index: &self.chain_index,
            token_contract_address: &token_address,
        }])
        .map_err(|e| format!("encode body: {e}"))?;
        let url = format!("{}{PRICE_PATH}", self.base_url);
        let timestamp = iso_timestamp();
        let sign = sign_request(
            &self.secret_key,
            &timestamp,
            "POST",
            PRICE_PATH,
            &body,
        )?;
        let response: Value = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("OK-ACCESS-KEY", &self.api_key)
            .header("OK-ACCESS-SIGN", sign)
            .header("OK-ACCESS-TIMESTAMP", &timestamp)
            .header("OK-ACCESS-PASSPHRASE", &self.passphrase)
            .body(body)
            .send()
            .map_err(|e| format!("request failed: {e}"))?
            .error_for_status()
            .map_err(|e| format!("http error: {e}"))?
            .json()
            .map_err(|e| format!("bad json: {e}"))?;
        price_from_response(&response, decimals)
    }
}

impl PriceSource for Okx {
    fn price_minor(&self, symbol: &str, decimals: u32) -> Result<u64, String> {
        self.price(symbol, decimals)
    }
}

/// UTC ISO-8601 timestamp with millisecond precision for `OK-ACCESS-TIMESTAMP`.
#[must_use]
pub fn iso_timestamp() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// `base64(hmac_sha256(timestamp + method + requestPath + body, secret_key))`.
pub fn sign_request(
    secret_key: &str,
    timestamp: &str,
    method: &str,
    request_path: &str,
    body: &str,
) -> Result<String, String> {
    let prehash = format!("{timestamp}{method}{request_path}{body}");
    let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
        .map_err(|e| format!("hmac key: {e}"))?;
    mac.update(prehash.as_bytes());
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

/// Extract the first `data[].price` from an OKX market price response.
pub fn price_from_response(body: &Value, decimals: u32) -> Result<u64, String> {
    let code = body["code"]
        .as_str()
        .or_else(|| body["code"].as_i64().map(|_| "0"));
    if code != Some("0") {
        let msg = body["msg"].as_str().unwrap_or("unknown error");
        let code = code.unwrap_or("?");
        return Err(format!("okx error {code}: {msg}"));
    }
    let price = body["data"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["price"].as_str())
        .ok_or_else(|| format!("missing data[0].price in response: {body}"))?;
    parse_decimal_to_minor(price, decimals)
        .ok_or_else(|| format!("unparseable price {price:?}"))
}

/// EVM addresses are lowercased per OKX docs; other formats are left as-is.
fn normalize_token_address(address: &str) -> String {
    if address.starts_with("0x") || address.starts_with("0X") {
        address.to_ascii_lowercase()
    } else {
        address.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{iso_timestamp, price_from_response, sign_request, PRICE_PATH};
    use serde_json::json;

    #[test]
    fn sign_is_deterministic() {
        let sign = sign_request(
            "22582BD0CFF14C41EDBF1AB98506286D",
            "2020-12-08T09:08:57.715Z",
            "GET",
            "/api/v5/account/balance?ccy=BTC",
            "",
        )
        .unwrap();
        assert_eq!(sign.len(), 44);
        assert_eq!(
            sign,
            sign_request(
                "22582BD0CFF14C41EDBF1AB98506286D",
                "2020-12-08T09:08:57.715Z",
                "GET",
                "/api/v5/account/balance?ccy=BTC",
                "",
            )
            .unwrap()
        );
    }

    #[test]
    fn sign_includes_post_body() {
        let body = r#"[{"chainIndex":"501","tokenContractAddress":"jup..."}]"#;
        let a = sign_request("secret", "ts", "POST", PRICE_PATH, body).unwrap();
        let b = sign_request("secret", "ts", "POST", PRICE_PATH, "").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn iso_timestamp_has_millis_and_zulu_suffix() {
        let ts = iso_timestamp();
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('.'));
    }

    #[test]
    fn extracts_price_from_success_response() {
        let body = json!({
            "code": "0",
            "data": [{
                "chainIndex": "501",
                "tokenContractAddress": "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN",
                "time": "1716892020000",
                "price": "1.234567"
            }],
            "msg": ""
        });
        assert_eq!(price_from_response(&body, 6), Ok(1_234_567));
    }

    #[test]
    fn api_error_surfaces_msg() {
        let body = json!({ "code": "50113", "msg": "Invalid sign", "data": [] });
        assert!(price_from_response(&body, 6).unwrap_err().contains("Invalid sign"));
    }

    #[test]
    fn missing_price_errors() {
        let body = json!({ "code": "0", "data": [{}], "msg": "" });
        assert!(price_from_response(&body, 6).is_err());
    }
}
