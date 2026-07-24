use std::collections::HashMap;

use crate::portfolio::{self, HoldingInput, PortfolioBrief};
use crate::rpc;
use crate::token_risk::{self, Concentration, Extension, MintCore, Risk, RiskReport, Signal};
use crate::tokens;

const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const MAX_TOKENS: usize = 100;
const MAX_RISK_LOOKUPS: usize = 40;

pub trait Http {
    fn rpc(&self, body: &str) -> Result<String, String>;
    fn get(&self, url: &str) -> Result<String, String>;
}

pub fn validate_wallet(wallet: &str) -> Result<(), String> {
    let bytes = bs58::decode(wallet)
        .into_vec()
        .map_err(|_| "wallet is not valid base58".to_string())?;
    if bytes.len() != 32 {
        return Err(format!("wallet must decode to 32 bytes, got {}", bytes.len()));
    }
    Ok(())
}

pub fn run(http: &dyn Http, wallet: &str, min_usd: f64) -> Result<PortfolioBrief, String> {
    validate_wallet(wallet)?;

    let sol = rpc::parse_balance_response(&http.rpc(&rpc::build_balance_request(wallet))?)?;

    let mut raw = rpc::parse_token_accounts_response(
        &http.rpc(&rpc::build_token_accounts_request(wallet, token_risk::TOKEN_PROGRAM_ID))?,
    )?;
    raw.extend(rpc::parse_token_accounts_response(
        &http.rpc(&rpc::build_token_accounts_request(wallet, token_risk::TOKEN_2022_PROGRAM_ID))?,
    )?);

    let truncated = raw.len() > MAX_TOKENS;
    raw.truncate(MAX_TOKENS);

    let mut ids: Vec<String> = raw.iter().map(|h| h.mint.clone()).collect();
    ids.push(SOL_MINT.to_string());
    let mut meta: HashMap<String, tokens::TokenMeta> = HashMap::new();
    for chunk in ids.chunks(tokens::MAX_IDS_PER_REQUEST) {
        meta.extend(tokens::parse_tokens_response(&http.get(&tokens::build_tokens_url(chunk))?)?);
    }
    let sol_price = meta.get(SOL_MINT).and_then(|m| m.usd_price).unwrap_or(0.0);
    let sol_change = meta.get(SOL_MINT).and_then(|m| m.change_24h);

    let mut scored: Vec<(rpc::TokenHolding, f64)> = raw
        .into_iter()
        .map(|h| {
            let usd = meta
                .get(&h.mint)
                .and_then(|m| m.usd_price)
                .map(|p| h.ui_amount * p)
                .unwrap_or(0.0);
            (h, usd)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut inputs = Vec::new();
    let mut budget = MAX_RISK_LOOKUPS;
    let mut unpriced_dangerous = 0usize;
    for (h, usd) in scored {
        let m = meta.get(&h.mint);
        let priced = m.and_then(|x| x.usd_price).is_some();
        let eligible = budget > 0 && (if priced { usd >= min_usd } else { true });
        let (risk, reason) = if eligible {
            budget -= 1;
            assess_risk(http, &h.mint, m)
        } else {
            (Risk::Green, String::new())
        };
        if !priced && risk == Risk::Red {
            unpriced_dangerous += 1;
        }
        let symbol = m
            .map(|x| x.symbol.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| short_mint(&h.mint));
        inputs.push(HoldingInput {
            symbol,
            mint: h.mint,
            amount: h.ui_amount,
            price_usd: m.and_then(|x| x.usd_price),
            price_change_24h: m.and_then(|x| x.change_24h),
            risk,
            risk_reason: reason,
        });
    }

    let mut brief = portfolio::build_brief(sol, sol_price, sol_change, inputs, min_usd);
    if unpriced_dangerous > 0 {
        brief.flags.push(format!(
            "{unpriced_dangerous} unpriced token(s) look dangerous (active authority or non-transferable); possible airdrop scam"
        ));
    }
    if truncated {
        brief
            .flags
            .push(format!("large wallet: only the first {MAX_TOKENS} tokens were scanned"));
    }
    Ok(brief)
}

fn assess_risk(http: &dyn Http, mint: &str, meta: Option<&tokens::TokenMeta>) -> (Risk, String) {
    let conc = meta
        .and_then(|m| m.top_holders_pct)
        .map(|p| Concentration { top1_pct: 0.0, top10_pct: p });

    let verified = meta.map(|m| m.is_verified).unwrap_or(false);

    let mut report: RiskReport = match fetch_mint(http, mint) {
        Ok((core, exts)) => token_risk::score(&core, &exts, conc.as_ref(), verified),
        Err(why) => RiskReport {
            risk: Risk::Amber,
            signals: vec![Signal { severity: Risk::Amber, reason: format!("risk unknown ({why})") }],
        },
    };

    match meta {
        None => report
            .signals
            .push(Signal { severity: Risk::Amber, reason: "not on Jupiter".to_string() }),
        Some(m) if !m.is_verified => report
            .signals
            .push(Signal { severity: Risk::Amber, reason: "unverified".to_string() }),
        _ => {}
    }

    let risk = if report.signals.iter().any(|s| s.severity == Risk::Red) {
        Risk::Red
    } else if report.signals.iter().any(|s| s.severity == Risk::Amber) {
        Risk::Amber
    } else {
        Risk::Green
    };
    let reason = report
        .signals
        .iter()
        .take(2)
        .map(|s| s.reason.clone())
        .collect::<Vec<_>>()
        .join("; ");
    (risk, reason)
}

fn fetch_mint(http: &dyn Http, mint: &str) -> Result<(MintCore, Vec<Extension>), String> {
    let resp = http.rpc(&rpc::build_account_info_request(mint))?;
    let acc = rpc::parse_account_info_response(&resp)?;
    let core = token_risk::parse_mint(&acc.data)?;
    let exts = token_risk::parse_extensions(&acc.data);
    Ok((core, exts))
}

fn short_mint(mint: &str) -> String {
    if mint.len() > 9 {
        format!("{}…{}", &mint[..4], &mint[mint.len() - 4..])
    } else {
        mint.to_string()
    }
}

pub fn with_default_https_port(url: &str) -> String {
    let rest = match url.strip_prefix("https://") {
        Some(r) => r,
        None => return url.to_string(),
    };
    let (authority, tail) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if authority.contains(':') {
        url.to_string()
    } else {
        format!("https://{authority}:443{tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    struct FakeHttp;
    impl Http for FakeHttp {
        fn rpc(&self, body: &str) -> Result<String, String> {
            if body.contains("getBalance") {
                Ok(r#"{"result":{"value":1000000000}}"#.to_string())
            } else if body.contains("getTokenAccountsByOwner") {
                if body.contains("Tokenkeg") {
                    Ok(r#"{"result":{"value":[{"account":{"data":{"parsed":{"info":{"mint":"MintA","tokenAmount":{"uiAmount":100.0}}}}}}]}}"#.to_string())
                } else {
                    Ok(r#"{"result":{"value":[]}}"#.to_string())
                }
            } else if body.contains("getAccountInfo") {
                let b64 = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 82]);
                Ok(format!(
                    r#"{{"result":{{"value":{{"owner":"{}","data":["{}","base64"]}}}}}}"#,
                    token_risk::TOKEN_PROGRAM_ID, b64
                ))
            } else {
                Err("unexpected rpc call".to_string())
            }
        }
        fn get(&self, _url: &str) -> Result<String, String> {
            Ok(format!(
                r#"[{{"id":"MintA","symbol":"AAA","usdPrice":2.0,"isVerified":true,"stats24h":{{"priceChange":1.0}},"audit":{{"topHoldersPercentage":10.0}}}},{{"id":"{SOL_MINT}","symbol":"SOL","usdPrice":150.0,"isVerified":true,"stats24h":{{"priceChange":0.5}}}}]"#
            ))
        }
    }

    #[test]
    fn run_end_to_end_with_mock() {
        let brief = run(&FakeHttp, SOL_MINT, 1.0).unwrap();
        assert!((brief.total_usd - 350.0).abs() < 1e-6);
        assert_eq!(brief.holdings.len(), 1);
        assert_eq!(brief.holdings[0].symbol, "AAA");
        assert!((brief.holdings[0].usd_value - 200.0).abs() < 1e-6);
        assert_eq!(brief.holdings[0].risk, Risk::Green);
    }

    #[test]
    fn rejects_bad_wallet_before_network() {
        assert!(run(&FakeHttp, "not a valid wallet !!!", 1.0).is_err());
    }

    #[test]
    fn adds_default_https_port() {
        assert_eq!(
            with_default_https_port("https://api.mainnet-beta.solana.com"),
            "https://api.mainnet-beta.solana.com:443"
        );
        assert_eq!(
            with_default_https_port("https://lite-api.jup.ag/tokens/v2/search?query=A,B"),
            "https://lite-api.jup.ag:443/tokens/v2/search?query=A,B"
        );
        assert_eq!(with_default_https_port("https://x.io:443/y"), "https://x.io:443/y");
        assert_eq!(with_default_https_port("http://h/x"), "http://h/x");
    }

    struct FakeManyHttp;
    impl Http for FakeManyHttp {
        fn rpc(&self, body: &str) -> Result<String, String> {
            if body.contains("getBalance") {
                Ok(r#"{"result":{"value":0}}"#.to_string())
            } else if body.contains("getTokenAccountsByOwner") {
                if body.contains("Tokenkeg") {
                    let items: Vec<String> = (0..120)
                        .map(|i| format!(
                            r#"{{"account":{{"data":{{"parsed":{{"info":{{"mint":"Mint{i}","tokenAmount":{{"uiAmount":100.0}}}}}}}}}}}}"#
                        ))
                        .collect();
                    Ok(format!(r#"{{"result":{{"value":[{}]}}}}"#, items.join(",")))
                } else {
                    Ok(r#"{"result":{"value":[]}}"#.to_string())
                }
            } else {
                Ok(r#"{"result":{"value":null}}"#.to_string())
            }
        }
        fn get(&self, _url: &str) -> Result<String, String> {
            Ok("[]".to_string())
        }
    }

    #[test]
    fn caps_large_wallet_and_flags_truncation() {
        let brief = run(&FakeManyHttp, SOL_MINT, 1.0).unwrap();
        assert!(brief.flags.iter().any(|f| f.contains("large wallet")));
    }
}
