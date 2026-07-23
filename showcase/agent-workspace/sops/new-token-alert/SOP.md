## Steps

1. **Snapshot** - Get the current brief for the watched wallet as JSON.
   - tools: portfolio_brief
   - input: {"type":"object","required":["wallet"],"properties":{"wallet":{"type":"string"},"format":{"type":"string"}}}

2. **Compare** - Diff this snapshot against the last one saved in memory. Worth a
   ping: a mint that was not there before, a holding that is now red, or the risky
   dollar amount up by more than the operator's threshold.

3. **Decide** - If nothing worth a ping changed, send no message at all. Only if
   something changed, post a short Telegram note naming what changed and why, then
   save this snapshot to memory as the new baseline.
