# Portfolio Guardian: a quiet read-only Solana watchdog on Telegram

I hold SOL and a pile of SPL tokens across a couple of wallets, and two things
kept bugging me: checking them means opening a bunch of explorer tabs, and every
week some token I never bought shows up airdropped into a wallet, half of them
scams. So I built a Telegram bot to watch the wallets for me.

The point is not a daily report you learn to ignore. Most mornings it says
nothing. It pings me only when something changed: a new token landed, or the
risky slice of the wallet grew. When I ask it directly, it sends a short brief.

Demo (under 3 min): <link>
Code and config: https://github.com/Alberluk1/portfolio-guardian

## What it does

DM it a wallet address and it replies with a brief: total value, how much sits in
safe tokens versus risky ones in dollars (that is the number I actually look at),
the top holdings with 24h change, and a 🔴 / 🟡 / 🟢 flag on each token with a
one-line reason. Say "watch this wallet <address>" and it remembers the wallet,
checks it on a schedule, and stays quiet unless a new dangerous token appears or
the risky dollar amount jumps.

```
💼 Portfolio 9WzD…AWWM: $12,480.55 (-0.5% 24h)
🟢 safe $11.7K · ⚠️ risky $0.8K
◎ SOL: $8,120.40
🟢 JTO: $3,560.00 (-1.6% 24h)
🟡 WIF: $780.15 (+4.2% 24h)  unverified on Jupiter
🔴 FOO: $220.00  Mint authority is active: an unknown, unverified creator can print unlimited new supply.
⚠️ 1 holding(s) red-flagged
```

There are other risk scanners in this batch. This one is meant to sit there and
watch, not be a lookup you run once and forget.

## Who it is for

People who hold a handful of Solana tokens and want to know, without having to
think about it, when something in their wallet turned risky. No trading, no
signing.

## ZeroClaw features it uses

Telegram over long polling (no public URL, runs on a home box). One tool plugin.
One skill. Two SOPs: a quiet new-token alert that is the default and stays silent
unless something changed, and an optional morning brief for people who want one.
Memory for the watched wallet. A read-only risk profile, and a runtime cap on
tool output so a big wallet cannot flood the model context.

## What I built (the plugin)

I tried doing this as a skill over the built-in http tool first. That is fine
until you hit Token-2022 tokens, where the risk that matters (a permanent
delegate that can claw back balances, a transfer hook, freeze-by-default) sits in
a TLV blob you have to walk byte by byte. That walk is the plugin. No solana-sdk;
it reads the 82-byte mint and the extension list by hand. Risk is two signals and
the worse one wins: what I parse on-chain, and whether Jupiter has the token
verified.

So this is a Tier 3 plugin, not Tier 1, for two reasons. One is the byte parsing
above. The other is that a skill cannot promise a token name is scrubbed before
the model sees it, because the model is the thing reading the raw tool output. In
the plugin the sanitize runs inside the tool, before anything leaves it.

One bit I am a little proud of: it does not cry wolf on USDC or USDT. Those keep a
mint and freeze authority on purpose, so a verified issuer with an authority shows
amber with that reason, while an unknown token with the same authority is red.
PYUSD still comes out red, because it carries a permanent delegate, and a golden
test on the real mainnet bytes proves the parser catches it.

44 host tests, including that golden test and property tests that throw random
bytes at the parser and random strings at the name sanitizer.

## Custody: T0, read-only

No key, nothing that signs or sends. The plugin can do exactly two things: make
outbound read calls (Solana RPC, Jupiter) and read its own config for an rpc_url.
Even on a full hijack there is no tool that moves money.

What actually needs defending at T0 is the data. Anyone can mint a token with any
name and drop it in a wallet, and that name goes to the model, so the tool scrubs
names before they leave it. See [demo/prompt-injection-transcript.md](demo/prompt-injection-transcript.md).

## What bit me

An https URL with no port just fails from inside a plugin. I spent an evening sure
my RPC parser was broken before I realized the request was dialing port 80,
because the default 443 does not survive the waki to wasi:http hop. One line to
normalize the URL to :443 and it worked. Two more things that cost me time if you
reproduce this: plugins are not in the release binary (you build the host with
`--features plugins-wasm-cranelift`), and `plugins.enabled` is false by default,
so an installed plugin's tools silently never reach the agent until you flip it.

Three runtime settings turned out to be load-bearing, and the config example
carries all three. The model has to be told to use native tool-calling
(`native_tools = true`), or a small model just answers from its own head and
never calls the tool. The reply-intent precheck classifies a bare wallet address
as "chatter" and stays silent, so it has to be off. And history has to be capped
(`max_history_messages = 1`), or on a second query the model blends the previous
wallet's numbers into the new answer. That last one is the one that matters for a
read-only tool: a hallucinated balance is worse than no balance, so each query is
kept independent.

## Known limits

Concentration comes from Jupiter's audit number, not my own on-chain holder scan.
Doing it on-chain properly means telling an AMM pool apart from a real whale, since
the biggest holder of most liquid tokens is a Raydium or Orca pool, and I did not
want to ship a signal that flags SOL as concentrated. That one is future work.

Wallets with more than about a hundred token accounts are scanned best-effort. The
token-accounts RPC hands back everything in one response with no server-side limit,
so a wallet stuffed with thousands of dust tokens is capped and flagged rather than
scanned in full.

## Reproduce

1. Build the plugin: `cargo build --target wasm32-wasip2 --release`. The component
   is `portfolio_brief.wasm`.
2. Build the host from source with plugin and Telegram support (confirm the exact
   feature names for your checkout, e.g. `plugins-wasm-cranelift` and the Telegram
   channel feature).
3. Install the plugin locally (`portfolio_brief.wasm` next to its `manifest.toml`)
   and set `plugins.enabled = true`.
4. Copy `config.example.toml` to your ZeroClaw config. Set the Telegram bot token
   with `zeroclaw config set` (never in plaintext). Optionally set an `rpc_url`
   with a Helius or Triton key.
5. Copy `agent-workspace/` (AGENTS.md, SOUL.md, `skills/`, `sops/`) into the
   `guardian` agent workspace.
6. Run a local model (`ollama pull qwen3:4b-instruct-2507-q4_K_M`) or point
   `model_provider` at any provider you have, then `zeroclaw daemon`.
7. DM the bot a Solana wallet address.

The `daily-brief` and `new-token-alert` SOPs use a cron trigger; confirm the exact
schedule field against `zeroclaw config schema` on your build, since the docs
render that block through a preprocessor.
