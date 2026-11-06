# FUKILLA — Hackathon Submission

FUKILLA — a self-enforcing, serverless security layer for autonomous AI agent funds.

Each task gets its own sub-account. The agent gets one
function-call key. Rules live inside the contract: spend
cap, velocity breaker, canary honeypot, expiry. If the
agent is compromised, it quarantines itself. Funds always
return to the owner.

During development, adversarial review found two critical
bugs: transfers were leaving the factory balance instead of
the task sub-account (isolation was false at protocol level),
and the refund callback always sent 0 NEAR. Both fixed in
commit 13a067d, documented publicly.

Built on pure NEAR primitives: sub-accounts, function-call
keys, cross-contract promises. Any NEAR project giving AI
agents money can integrate FUKILLA in 3 steps — no new
token, no new infrastructure.

Track 06: spending limits, sandboxed wallets, function-call
keys — exactly as specified in the track prompt.
