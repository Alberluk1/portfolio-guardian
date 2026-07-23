use std::process::Command;

use portfolio_brief::engine::{run, Http};
use portfolio_brief::portfolio::render_human;

struct CurlHttp {
    rpc_url: String,
}

impl Http for CurlHttp {
    fn rpc(&self, body: &str) -> Result<String, String> {
        let out = Command::new("curl")
            .args([
                "-s", &self.rpc_url, "-X", "POST",
                "-H", "Content-Type: application/json",
                "-d", body,
            ])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }
    fn get(&self, url: &str) -> Result<String, String> {
        let out = Command::new("curl")
            .args(["-s", url])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }
}

fn main() {
    let wallet = std::env::args()
        .nth(1)
        .expect("usage: cargo run --example live -- <WALLET>");
    let key = std::env::var("HELIUS_KEY").unwrap_or_default();
    let rpc_url = if key.is_empty() {
        "https://api.mainnet-beta.solana.com".to_string()
    } else {
        format!("https://mainnet.helius-rpc.com/?api-key={key}")
    };

    let short = if wallet.len() > 9 {
        format!("{}…{}", &wallet[..4], &wallet[wallet.len() - 4..])
    } else {
        wallet.clone()
    };

    match run(&CurlHttp { rpc_url }, &wallet, 1.0) {
        Ok(brief) => println!("\n{}\n", render_human(&brief, &short)),
        Err(e) => eprintln!("error: {e}"),
    }
}
