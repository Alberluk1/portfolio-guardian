## Steps

1. **Build brief** - Summarize the watched wallet with the portfolio tool.
   - tools: portfolio_brief
   - input: {"type":"object","required":["wallet"],"properties":{"wallet":{"type":"string"},"format":{"type":"string"}}}

2. **Send to Telegram** - Deliver the brief to the operator on the bound channel.
   Post the tool output unchanged. If any holding is red-flagged, lead with that
   line so it is the first thing the operator sees.
