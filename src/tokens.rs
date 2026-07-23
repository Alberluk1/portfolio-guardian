use serde_json::Value;
use std::collections::HashMap;

pub const JUP_TOKENS_BASE: &str = "https://lite-api.jup.ag/tokens/v2/search";
pub const MAX_IDS_PER_REQUEST: usize = 50;

#[derive(Debug, Clone, PartialEq)]
pub struct TokenMeta {
    pub symbol: String,
    pub usd_price: Option<f64>,
    pub change_24h: Option<f64>,
    pub is_verified: bool,
    pub top_holders_pct: Option<f64>,
}

pub fn build_tokens_url(mints: &[String]) -> String {
    format!("{JUP_TOKENS_BASE}?query={}", mints.join(","))
}

pub fn parse_tokens_response(json: &str) -> Result<HashMap<String, TokenMeta>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("bad tokens JSON: {e}"))?;
    let arr = v.as_array().ok_or("tokens response is not a JSON array")?;
    let mut out = HashMap::new();
    for t in arr {
        let id = match t.get("id").and_then(|x| x.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        out.insert(
            id,
            TokenMeta {
                symbol: t
                    .get("symbol")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string(),
                usd_price: t.get("usdPrice").and_then(|x| x.as_f64()),
                change_24h: t
                    .get("stats24h")
                    .and_then(|s| s.get("priceChange"))
                    .and_then(|x| x.as_f64()),
                is_verified: t.get("isVerified").and_then(|x| x.as_bool()).unwrap_or(false),
                top_holders_pct: t
                    .get("audit")
                    .and_then(|a| a.get("topHoldersPercentage"))
                    .and_then(|x| x.as_f64()),
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_batch_url() {
        assert_eq!(
            build_tokens_url(&["A".to_string(), "B".to_string()]),
            "https://lite-api.jup.ag/tokens/v2/search?query=A,B"
        );
    }

    #[test]
    fn parses_real_shape() {
        let json = r#"[
          {"id":"USDTmint","symbol":"USDT","name":"USDT","usdPrice":0.999,"isVerified":true,
           "stats24h":{"priceChange":0.0044},"audit":{"topHoldersPercentage":34.7}},
          {"id":"SOLmint","symbol":"SOL","usdPrice":77.6,"isVerified":true,
           "stats24h":{"priceChange":-0.40},"audit":{"topHoldersPercentage":0.58}}
        ]"#;
        let m = parse_tokens_response(json).unwrap();
        let usdt = m.get("USDTmint").unwrap();
        assert_eq!(usdt.symbol, "USDT");
        assert!(usdt.is_verified);
        assert!((usdt.usd_price.unwrap() - 0.999).abs() < 1e-6);
        assert!((usdt.change_24h.unwrap() - 0.0044).abs() < 1e-6);
        assert!((usdt.top_holders_pct.unwrap() - 34.7).abs() < 1e-6);
    }

    #[test]
    fn absent_mint_is_not_in_map() {
        assert!(parse_tokens_response("[]").unwrap().is_empty());
    }
}
