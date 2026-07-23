use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct AccountInfo {
    pub owner: String,
    pub data: Vec<u8>,
}

pub fn build_account_info_request(mint: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getAccountInfo",
        "params": [mint, { "encoding": "base64" }]
    })
    .to_string()
}

pub fn parse_account_info_response(json: &str) -> Result<AccountInfo, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("bad JSON: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("RPC error: {err}"));
    }
    let value = v
        .get("result")
        .and_then(|r| r.get("value"))
        .ok_or("response has no result.value")?;
    if value.is_null() {
        return Err("account not found (mint may not exist)".to_string());
    }
    let owner = value
        .get("owner")
        .and_then(|o| o.as_str())
        .ok_or("response has no owner")?
        .to_string();
    let b64 = value
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|s| s.as_str())
        .ok_or("response has no base64 data")?;
    let data = STANDARD
        .decode(b64)
        .map_err(|e| format!("bad base64 account data: {e}"))?;
    Ok(AccountInfo { owner, data })
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenHolding {
    pub mint: String,
    pub ui_amount: f64,
}

pub fn build_token_accounts_request(owner: &str, program_id: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "getTokenAccountsByOwner",
        "params": [owner, { "programId": program_id }, { "encoding": "jsonParsed" }]
    })
    .to_string()
}

pub fn parse_token_accounts_response(json: &str) -> Result<Vec<TokenHolding>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("bad JSON: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("RPC error: {err}"));
    }
    let arr = v
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(|x| x.as_array())
        .ok_or("no result.value array")?;
    let mut out = Vec::new();
    for acc in arr {
        let info = acc
            .get("account")
            .and_then(|a| a.get("data"))
            .and_then(|d| d.get("parsed"))
            .and_then(|p| p.get("info"));
        let info = match info {
            Some(i) => i,
            None => continue,
        };
        let mint = match info.get("mint").and_then(|m| m.as_str()) {
            Some(m) => m.to_string(),
            None => continue,
        };
        let ui = info
            .get("tokenAmount")
            .and_then(|t| t.get("uiAmount"))
            .and_then(|u| u.as_f64())
            .unwrap_or(0.0);
        if ui > 0.0 {
            out.push(TokenHolding { mint, ui_amount: ui });
        }
    }
    Ok(out)
}

pub fn build_balance_request(owner: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "getBalance", "params": [owner]
    })
    .to_string()
}

pub fn parse_balance_response(json: &str) -> Result<f64, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("bad JSON: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("RPC error: {err}"));
    }
    let lamports = v
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(|x| x.as_u64())
        .ok_or("no result.value (lamports)")?;
    Ok(lamports as f64 / 1_000_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_valid_request() {
        let body = build_account_info_request("So11111111111111111111111111111111111111112");
        assert!(body.contains("getAccountInfo"));
        assert!(body.contains("So11111111111111111111111111111111111111112"));
        assert!(body.contains("base64"));
    }

    #[test]
    fn parses_account_info() {
        let json = r#"{
            "jsonrpc":"2.0","id":1,
            "result":{"context":{"slot":1},"value":{
                "owner":"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
                "data":["AQID","base64"],
                "lamports":1
            }}
        }"#;
        let info = parse_account_info_response(json).unwrap();
        assert_eq!(info.owner, "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
        assert_eq!(info.data, vec![1, 2, 3]);
    }

    #[test]
    fn handles_missing_account() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":null}}"#;
        assert!(parse_account_info_response(json).is_err());
    }

    #[test]
    fn handles_rpc_error() {
        let json = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"bad"}}"#;
        assert!(parse_account_info_response(json).is_err());
    }

    #[test]
    fn parses_token_accounts_skips_zero() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"value":[
            {"account":{"data":{"parsed":{"info":{"mint":"MintA","tokenAmount":{"uiAmount":12.5,"decimals":6}}}}}},
            {"account":{"data":{"parsed":{"info":{"mint":"MintZero","tokenAmount":{"uiAmount":0,"decimals":0}}}}}}
        ]}}"#;
        let h = parse_token_accounts_response(json).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].mint, "MintA");
        assert!((h[0].ui_amount - 12.5).abs() < 1e-9);
    }

    #[test]
    fn parses_balance_lamports_to_sol() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":2500000000}}"#;
        let sol = parse_balance_response(json).unwrap();
        assert!((sol - 2.5).abs() < 1e-9);
    }
}
