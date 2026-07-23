pub mod token_risk;
pub mod rpc;
pub mod portfolio;
pub mod tokens;
pub mod engine;

#[cfg(target_family = "wasm")]
mod component {
    wit_bindgen::generate!({
        path: "wit/v0",
        world: "tool-plugin",
        features: ["plugins-wit-v0"],
    });

    use std::collections::HashMap;

    use serde_json::Value;

    use crate::engine::{self, Http};
    use crate::portfolio;

    use exports::zeroclaw::plugin::plugin_info::Guest as PluginInfo;
    use exports::zeroclaw::plugin::tool::{Guest as Tool, ToolResult};
    use zeroclaw::plugin::logging::{
        log_record, LogLevel, PluginAction, PluginEvent, PluginOutcome,
    };

    const PLUGIN_NAME: &str = "portfolio-brief";
    const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
    const TOOL_NAME: &str = "portfolio_brief";
    const DEFAULT_RPC: &str = "https://api.mainnet-beta.solana.com";

    struct WakiHttp {
        rpc_url: String,
    }

    impl Http for WakiHttp {
        fn rpc(&self, body_str: &str) -> Result<String, String> {
            let body: Value = serde_json::from_str(body_str).map_err(|e| e.to_string())?;
            let url = engine::with_default_https_port(&self.rpc_url);
            let v = waki::Client::new()
                .post(&url)
                .json(&body)
                .send()
                .map_err(|e| e.to_string())?
                .json::<Value>()
                .map_err(|e| e.to_string())?;
            Ok(v.to_string())
        }

        fn get(&self, url: &str) -> Result<String, String> {
            let url = engine::with_default_https_port(url);
            let v = waki::Client::new()
                .get(&url)
                .send()
                .map_err(|e| e.to_string())?
                .json::<Value>()
                .map_err(|e| e.to_string())?;
            Ok(v.to_string())
        }
    }

    #[derive(serde::Deserialize)]
    struct ExecuteArgs {
        wallet: String,
        #[serde(default)]
        format: Option<String>,
        #[serde(rename = "__config", default)]
        config: HashMap<String, String>,
    }

    fn short_wallet(w: &str) -> String {
        if w.len() > 9 {
            format!("{}…{}", &w[..4], &w[w.len() - 4..])
        } else {
            w.to_string()
        }
    }

    fn emit(action: PluginAction, outcome: PluginOutcome, message: &str) {
        log_record(
            LogLevel::Info,
            &PluginEvent {
                function_name: "portfolio_brief::tool::execute".to_string(),
                action,
                outcome: Some(outcome),
                duration_ms: None,
                attrs: None,
                message: message.to_string(),
            },
        );
    }

    struct PortfolioBriefTool;

    impl PluginInfo for PortfolioBriefTool {
        fn plugin_name() -> String {
            PLUGIN_NAME.to_string()
        }
        fn plugin_version() -> String {
            PLUGIN_VERSION.to_string()
        }
    }

    impl Tool for PortfolioBriefTool {
        fn name() -> String {
            TOOL_NAME.to_string()
        }

        fn description() -> String {
            "Summarize a Solana wallet: total USD value, top holdings with 24h change, and a \
             red/amber/green risk flag per token (mint/freeze authority, Token-2022 extensions, \
             holder concentration). Read-only; give it a wallet address."
                .to_string()
        }

        fn parameters_schema() -> String {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "wallet": {
                        "type": "string",
                        "description": "Solana wallet address (base58) to summarize."
                    },
                    "format": {
                        "type": "string",
                        "enum": ["human", "json"],
                        "description": "Output style: 'human' (default, chat-friendly text) or 'json' (stable, schema-versioned; for automated diff/alerts)."
                    }
                },
                "required": ["wallet"]
            })
            .to_string()
        }

        fn execute(args: String) -> Result<ToolResult, String> {
            let parsed: ExecuteArgs = match serde_json::from_str(&args) {
                Ok(a) => a,
                Err(e) => {
                    emit(PluginAction::Fail, PluginOutcome::Failure, "invalid arguments");
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("invalid arguments: {e}")),
                    });
                }
            };

            let rpc_url = parsed
                .config
                .get("rpc_url")
                .cloned()
                .unwrap_or_else(|| DEFAULT_RPC.to_string());
            let min_usd = parsed
                .config
                .get("dust_threshold_usd")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(1.0);
            let http = WakiHttp { rpc_url };

            match engine::run(&http, &parsed.wallet, min_usd) {
                Ok(brief) => {
                    let short = short_wallet(&parsed.wallet);
                    let out = match parsed.format.as_deref() {
                        Some("json") => portfolio::render_json(&brief, &short),
                        _ => portfolio::render_human(&brief, &short),
                    };
                    emit(PluginAction::Complete, PluginOutcome::Success, "portfolio brief built");
                    Ok(ToolResult { success: true, output: out, error: None })
                }
                Err(e) => {
                    emit(PluginAction::Fail, PluginOutcome::Failure, "run failed");
                    Ok(ToolResult { success: false, output: String::new(), error: Some(e) })
                }
            }
        }
    }

    export!(PortfolioBriefTool);
}
