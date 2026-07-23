# Portfolio Guardian

A quiet, read-only Solana wallet watchdog for ZeroClaw. It runs as a self-hosted
Telegram agent: DM it a wallet and it replies with a risk-annotated brief, or tell
it to watch a wallet and it stays silent until a dangerous token shows up. Custody
tier T0: it holds no key and moves no funds.

The running agent (config, workspace, skill, SOPs, threat model, demo script) is in
[`showcase/`](showcase/). The rest of this README documents the tool plugin it is
built on, `portfolio-brief`.

## The plugin

Give it a Solana wallet address and it returns a compact, risk-annotated
portfolio: total USD value, top holdings with 24h change, and a red/amber/green
risk flag per token. Read-only (custody tier T0): it reads over `http_client`,
never holds a key and never signs.

## What it does

It combines three on-chain reads with one keyless metadata call:

- SOL balance (`getBalance`) and all token accounts, classic and Token-2022
  (`getTokenAccountsByOwner`).
- Symbol, USD price, 24h change, verification flag and holder-concentration audit
  from the Jupiter tokens API, in one batched request.
- Per-token risk: for the top holdings it reads the mint account
  (`getAccountInfo`) and parses it directly for mint/freeze authority, Token-2022
  extensions (transfer fee/hook, permanent delegate, non-transferable) and holder
  concentration.

Example output:

```
💼 Portfolio 9WzD…AWWM: $12,480.55 (-0.5% 24h)
🟢 safe $11.7K · ⚠️ risky $0.8K
◎ SOL: $8,120.40
🟢 JTO: $3,560.00 (-1.6% 24h)
🟡 WIF: $780.15 (+4.2% 24h)  unverified on Jupiter
🔴 FOO: $220.00 (+0.9% 24h)  Mint authority is active: an unknown, unverified creator can print unlimited new supply.
+ dust: $0.23 (hidden)
📈 WIF +4.2% · 📉 JTO -1.6% (24h)
⚠️ 1 holding(s) red-flagged
```

## Risk

Risk comes from two independent signals and the redder one wins: our own parsing
of the mint account, and Jupiter's verification flag. Verification puts an active
authority in context instead of hiding it. On an unknown token an active
mint/freeze authority is a rug vector (red); on a verified issuer like USDC or
USDT it is retained by design for compliance, so it shows amber with that reason.
A dangerous extension such as a permanent delegate stays red even on a verified
token; the golden test proves this against real PYUSD bytes.

## Output formats

`format: "human"` (default) renders the message above. `format: "json"` returns a
stable, `schema_version`-tagged object for automation, such as an alert routine
that diffs one run against the previous snapshot to spot a new token or a fresh
red flag. A full running agent (Telegram, config, skill, SOP, threat model) is in
[`showcase/`](showcase/).

## Config

| Key | Default | Meaning |
|---|---|---|
| `rpc_url` | `https://api.mainnet-beta.solana.com` | Solana RPC endpoint. Use a paid endpoint (Helius, Triton) for reliable `getTokenAccountsByOwner`; the public one rate-limits. |
| `dust_threshold_usd` | `1.0` | Holdings worth less than this are folded into a dust total instead of listed. Set by the operator, not by tool arguments. |

Config is optional and injected as `__config` only because the manifest requests
`config_read`. Without it the plugin runs on defaults.

## Safety

- Fail-closed: a hard error (bad wallet, RPC down) aborts instead of returning a
  partial portfolio that looks whole. Per-token risk fails soft to amber, never
  silently green.
- Token symbols are attacker-controlled, so they are sanitized (control,
  zero-width, RTL-override and markdown characters removed, length capped) before
  they can reach a chat or a model. Covered by an end-to-end test and property
  tests.
- Work is bounded for wallets with thousands of accounts, with an explicit flag
  when a wallet is truncated.
- T0 means no key ever touches this plugin.

## Build and test

```bash
cargo test
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/portfolio_brief.wasm portfolio_brief.wasm
```

Run it against a live wallet without deploying (uses `curl` as the HTTP client):

```bash
HELIUS_KEY=xxxx cargo run --example live -- <WALLET_ADDRESS>
```

## Install

Copy this directory (the `.wasm` next to its `manifest.toml`) into your configured
plugins dir and enable plugins:

```toml
[plugins]
enabled = true
```

Run the agent with a compiler backend, e.g.
`--features plugins-wasm,plugins-wasm-cranelift`. For runtime-only hosts, precompile
with a matching wasmtime and point `wasm_path` at the `.cwasm`.

## Limitations

No cost basis or realized P&L (needs transaction history), no LP/staking/lending
positions, no NFTs. High concentration and "unpriced" are cautions, not proof of a
rug: a large holder may be a liquidity pool, which is not disambiguated here.
