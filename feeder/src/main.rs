//! Minimal `doppler feeder` runner.
//!
//! parallel — SOL (Coinbase), ETH (Binance), ONYC (HttpJson), BNB (CoinGecko),
//! JUP (Jupiter), BTC (tokens.xyz), SUI (Birdeye) — every 60s, signing with the
//! Override with env vars:
//!   DOPPLER_RPC=<url>
//!   DOPPLER_ADMIN=<keypair.json>
//!   DOPPLER_INTERVAL_SECS=<n>
//!   COINGECKO_API_KEY=<key>    (optional; BNB feeder skipped if unset)
//!   JUPITER_API_KEY=<key>      (optional; JUP feeder skipped if unset)
//!   BIRDEYE_API_KEY=<key>      (optional; SUI feeder skipped if unset)
//!   TOKENS_XYZ_API_KEY=<key>   (optional; BTC feeder skipped if unset)
//!   OKX_API_KEY / OKX_SECRET_KEY / OKX_PASSPHRASE (optional; USDT feeder skipped if any unset)
//!
//! A proper `clap` CLI (`doppler init` / `doppler run`) is the next slice.

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use doppler_feeder::{Assets, Binance, Birdeye, CoinGecko, Coinbase, Feed, Feeder, HttpJson, Jupiter, Okx, TokensXyz, Variant};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::EncodableKey as _;

fn main() {
    // `.env` is not loaded by the OS; pick up repo-root `.env` when running via cargo.
    let _ = dotenvy::dotenv();

    let rpc_url = env_non_empty("DOPPLER_RPC").unwrap_or_else(|| "http://localhost:8899".to_string());

    let interval_secs: u64 = std::env::var("DOPPLER_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let interval = Duration::from_secs(interval_secs);

    let unit_price: u64 = 1_000;

    // Admin keypair the program expects. Defaults to the example key for surfpool.
    let admin_path: PathBuf = env_non_empty("DOPPLER_ADMIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            [
                env!("CARGO_MANIFEST_DIR"),
                "..",
                "examples",
                "keys",
                "admin-keypair.json",
            ]
            .iter()
            .collect()
        });

    let tokens_xyz_api_key = env_non_empty("TOKENS_XYZ_API_KEY");
    let birdeye_api_key = env_non_empty("BIRDEYE_API_KEY");
    let coingecko_api_key = env_non_empty("COINGECKO_API_KEY");
    let jupiter_api_key = env_non_empty("JUPITER_API_KEY");
    let okx_credentials = okx_credentials();

    // Derived from admin + seed + program id (see CLAP_CLI.md).
    let sol_oracle =
        Pubkey::from_str_const("QUVF91dzXWYvE5FmFEc41JZxRDmNgx8S8P6sNDWYZiW"); // SOL/USDC
    let eth_oracle =
        Pubkey::from_str_const("8ZCqkS8XhoPKXebkU5vRrpPHWMDi5LBaAk4DGZ8pNhpH"); // ETH/USD
    let onyc_oracle =
        Pubkey::from_str_const("GgatNhEKWqML7VLBzFUPTPpRxRErFb7hR8cLPgsoKrQS"); // ONYC/USD
    let bnb_oracle =
        Pubkey::from_str_const("Ei8ZdxmKXS9aEM6SoWAfY8z9CvHUwyhA4ao5TSLBVndT"); // BNB/USD
    let jup_oracle =
        Pubkey::from_str_const("AFVGWn264t6e6vAV2KpKVufjkihmJ7PQCjAdLVjBY39Y"); // JUP/USD
    let btc_oracle =
        Pubkey::from_str_const("Goe6WgovgFwywg9vpaSEEVm9onneyR7ot3tmzRCvDGS5"); // BTC/USD
    let sui_oracle =
        Pubkey::from_str_const("CS4WZhsUKBFoT9TYC61xi4fSMtoLer1dj3xizbKFsqbk"); // SUI/USD
    let usdt_oracle =
        Pubkey::from_str_const("FrMJ8Z1zKHSg75fTTsmVhWnTzP57Y1hdH5JzvWQb2VXh"); // USDT/USD

    let mut optional_feeders = String::new();
    if coingecko_api_key.is_some() {
        optional_feeders.push_str(", BNB (CoinGecko)");
    }
    if jupiter_api_key.is_some() {
        optional_feeders.push_str(", JUP (Jupiter)");
    }
    if tokens_xyz_api_key.is_some() {
        optional_feeders.push_str(", BTC (tokens.xyz)");
    }
    if birdeye_api_key.is_some() {
        optional_feeders.push_str(", SUI (Birdeye)");
    }
    if okx_credentials.is_some() {
        optional_feeders.push_str(", USDT (OKX)");
    }

    println!(
        "doppler feeder: SOL (Coinbase), ETH (Binance), ONYC (HttpJson){optional_feeders} every {interval_secs}s -> {rpc_url}"
    );

    thread::scope(|scope| {
        scope.spawn(|| {
            let admin = load_admin(&admin_path);
            let feeder = Feeder::new(
                &rpc_url,
                admin,
                Coinbase::usd(),
                vec![Feed::new("SOL", sol_oracle)],
                Some(unit_price),
            );
            println!("  SOL feeder: 1 feed(s)");
            feeder.run(interval);
        });

        scope.spawn(|| {
            let admin = load_admin(&admin_path);
            let feeder = Feeder::new(
                &rpc_url,
                admin,
                Binance::usdt(),
                vec![Feed::new("ETH", eth_oracle)],
                Some(unit_price),
            );
            println!("  ETH feeder: 1 feed(s)");
            feeder.run(interval);
        });

        scope.spawn(|| {
            let admin = load_admin(&admin_path);
            let source = HttpJson::get("https://core.api.onre.finance/data/live-nav").decimals(6);
            let feeder = Feeder::new(
                &rpc_url,
                admin,
                source,
                vec![Feed::new("ONYC", onyc_oracle)],
                Some(unit_price),
            );
            println!("  ONYC feeder: 1 feed(s)");
            feeder.run(interval);
        });

        if let Some(coingecko_api_key) = coingecko_api_key {
            let rpc_url = rpc_url.clone();
            let admin_path = admin_path.clone();
            scope.spawn(move || {
                let admin = load_admin(&admin_path);
                let feeder = Feeder::new(
                    &rpc_url,
                    admin,
                    CoinGecko::new(coingecko_api_key),
                    vec![Feed::with_asset_id("BNB", "binancecoin", bnb_oracle)],
                    Some(unit_price),
                );
                println!("  BNB feeder: 1 feed(s)");
                feeder.run(interval);
            });
        } else {
            eprintln!("  BNB feeder skipped: COINGECKO_API_KEY not set");
        }

        if let Some(jupiter_api_key) = jupiter_api_key {
            let rpc_url = rpc_url.clone();
            let admin_path = admin_path.clone();
            const JUP_MINT: &str = "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN";
            scope.spawn(move || {
                let admin = load_admin(&admin_path);
                let feeder = Feeder::new(
                    &rpc_url,
                    admin,
                    Jupiter::new(jupiter_api_key),
                    vec![Feed::with_asset_id("JUP", JUP_MINT, jup_oracle)],
                    Some(unit_price),
                );
                println!("  JUP feeder: 1 feed(s)");
                feeder.run(interval);
            });
        } else {
            eprintln!("  JUP feeder skipped: JUPITER_API_KEY not set");
        }

        if let Some(tokens_xyz_api_key) = tokens_xyz_api_key {
            let rpc_url = rpc_url.clone();
            let admin_path = admin_path.clone();
            scope.spawn(move || {
                let admin = load_admin(&admin_path);
                let source = TokensXyz::new(tokens_xyz_api_key)
                    .assets(Assets::list(["bitcoin"]))
                    .variant(Variant::Primary);
                let feeder = Feeder::new(
                    &rpc_url,
                    admin,
                    source,
                    vec![Feed::with_asset_id("BTC", "bitcoin", btc_oracle)],
                    Some(unit_price),
                );
                println!("  BTC feeder: 1 feed(s)");
                feeder.run(interval);
            });
        } else {
            eprintln!("  BTC feeder skipped: TOKENS_XYZ_API_KEY not set");
        }

        if let Some(birdeye_api_key) = birdeye_api_key {
            let rpc_url = rpc_url.clone();
            let admin_path = admin_path.clone();
            scope.spawn(move || {
                let admin = load_admin(&admin_path);
                let source = Birdeye::new(birdeye_api_key).with_chain("sui");
                let feeder = Feeder::new(
                    &rpc_url,
                    admin,
                    source,
                    vec![Feed::with_asset_id("SUI", "0x2::sui::SUI", sui_oracle)],
                    Some(unit_price),
                );
                println!("  SUI feeder: 1 feed(s)");
                feeder.run(interval);
            });
        } else {
            eprintln!("  SUI feeder skipped: BIRDEYE_API_KEY not set");
        }

        if let Some((okx_api_key, okx_secret_key, okx_passphrase)) = okx_credentials {
            let rpc_url = rpc_url.clone();
            let admin_path = admin_path.clone();
            scope.spawn(move || {
                let admin = load_admin(&admin_path);
                // Solana USDT on OKX market price (chain 501).
                const SOLANA_USDT_MINT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
                let source = Okx::new(okx_api_key, okx_secret_key, okx_passphrase).chain_index("501");
                let feeder = Feeder::new(
                    &rpc_url,
                    admin,
                    source,
                    vec![Feed::with_asset_id("USDT", SOLANA_USDT_MINT, usdt_oracle)],
                    Some(unit_price),
                );
                println!("  USDT feeder: 1 feed(s)");
                feeder.run(interval);
            });
        } else {
            eprintln!("  USDT feeder skipped: OKX_API_KEY, OKX_SECRET_KEY, and OKX_PASSPHRASE required");
        }
    });
}

/// Read an env var; returns `None` when unset or blank.
fn env_non_empty(var: &str) -> Option<String> {
    std::env::var(var).ok().and_then(|s| {
        let s = s.trim();
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    })
}

/// OKX signed requests need all three credentials.
fn okx_credentials() -> Option<(String, String, String)> {
    Some((
        env_non_empty("OKX_API_KEY")?,
        env_non_empty("OKX_SECRET_KEY")?,
        env_non_empty("OKX_PASSPHRASE")?,
    ))
}

fn load_admin(path: &PathBuf) -> Keypair {
    Keypair::read_from_file(path)
        .unwrap_or_else(|e| panic!("admin keypair not found at {}: {e}", path.display()))
}
