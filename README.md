# FUKILLA 🔥

**"Burn after paying."**

*"Burn after paying" — the wallet self-destructs after the task ends; all remaining funds return to the owner. Nothing is lost.*

FUKILLA is a security-first wallet factory on NEAR Protocol that gives AI agents money **without giving them the keys**.

Every AI agent that touches crypto has the same fatal design flaw: it holds a full private key to your funds. One leaked key, one prompt injection, one hallucination — everything gone. FUKILLA flips the model:

- The **owner** funds a fresh, disposable wallet per task ("a Fukilla") through the factory
- The **agent** receives exactly one **Function-Call access key**, scoped to a single method (`agent_call`), with a hard on-chain allowance (spend cap)
- The agent can work and pay — it can never own anything, never drain, never touch the owner's main account
- When the task completes, the balance returns to the owner and the wallet **deletes itself**

> Every transaction is its last.

## Why this matters

| Full-key agent wallet | Fukilla wallet |
|---|---|
| Key leak = total loss | Key leak = one capped allowance, nothing else to steal |
| Agent can spend everything | Allowance enforced at protocol level, not by trust |
| Keys live forever in logs/repos/screenshots | Key dies with the wallet after one task |
| Owner is directly exposed | The wallet is a cut-out: owner is never connected to the agent's transactions |

## Architecture

- **Factory contract** (`src/lib.rs`): `create_wallet` deploys a new sub-account with an embedded executor WASM, registers the task, and adds the agent's FC key with `Allowance::limited`
- **Executor contract** (`executor/`): minimal per-wallet logic — validates `agent_call` against the allowance, settles to the receiver, exposes `complete_task` (sweeps balance to owner, deletes the account) and `force_expire`
- **Demo tooling** (`demo/`): generates the agent FC keypair

### Live on NEAR testnet

- Factory: `fukilla.testnet`
- Demo wallet: `e2e-proof-2.fukilla.testnet` (funded with 1 NEAR, FC key capped at 0.1 NEAR)
- Create wallet tx: [`9ZCX8DJ2FPdGFaAmAt6aci9qWi2usdWLtkjMAmBYKugR`](https://testnet.nearblocks.io/tx/9ZCX8DJ2FPdGFaAmAt6aci9qWi2usdWLtkjMAmBYKugR)

### Security properties

1. **Least privilege** — the FC key can call exactly one method. Nothing else.
2. **On-chain spend cap** — allowance is enforced by the NEAR protocol itself, not by the agent's own discipline
3. **No ownership** — the agent key cannot transfer the account, deploy contracts, or change keys
4. **Self-destruct** — `complete_task` returns all remaining balance to the owner and deletes the account, so leaked keys become worthless
5. **Zero trust** — the owner never shares their main key with anyone, ever

## Bugs found & fixed during development

We publish these because a security project that claims to be flawless is the least credible one on the table.

1. **Payments came from the factory, not the task wallet.** In `agent_call`, the transfer was executed by the factory contract — meaning all tasks shared one pool of funds, contradicting the "isolated per-task wallet" claim. **Fix (commit `13a067d`):** the factory now forwards a `pay(receiver, amount)` call to the executor sub-account, which transfers from its own balance after verifying the caller is its parent factory. Isolation is now real at the protocol level.
2. **Refunds sent 0 NEAR.** The failure callback refunded `env::attached_deposit()` — which is always **0 inside a NEAR callback**, so a failed `create_wallet` silently kept the owner's deposit. **Fix (commit `13a067d`):** the attached deposit is stored in task state (`funding`) *before* the promise chain starts, and the callback refunds from that stored value.
3. **Minimum funding was too low (found live).** A 0.05 NEAR funding failed on-chain: deploying the executor WASM + storage staking costs a measured **0.845 NEAR**, and with only 1 NEAR the wallet's liquid balance after staking was too thin for the agent's gas. **Fix (commits `9996a16`, `13a067d`):** minimum funding raised to **2 NEAR**, proven by the sandbox integration test.
4. **Debugging lesson:** a `env::account_balance()` comparison inside the executor caused a silent wasm trap (panic=abort swallows messages); it was removed — the runtime already fails a transfer exceeding the wallet's balance, and the factory's spend cap bounds total outflow.

## Quickstart

### Prerequisites
- Rust + cargo (stable)
- NEAR CLI

### Run tests
```bash
cargo test -- --nocapture
```

### Run clippy
```bash
# Note: near-sdk v5 requires wasm32 target for clippy
cargo clippy --lib --target wasm32-unknown-unknown -- -D warnings
```

### Deploy to testnet
```bash
cargo near build && near deploy fukilla.testnet target/wasm32-unknown-unknown/release/fukilla.wasm
```

### Run attack simulation
```bash
cd demo && python3 attack_sim.py
```

> ⚠️ **Important:** `max_calls_per_window` defaults to 0 (disabled).
> Always set this explicitly when creating a task wallet.
> Example: `max_calls_per_window: 10, velocity_window_sec: 60`
> A wallet created without this parameter has no velocity protection.

## Setting Up the Canary (Honeypot)

The canary is a known-malicious address. If the agent ever
pays it, the wallet locks itself permanently and sweeps
funds back to the owner.

How to set it:
1. Deploy the factory with a known scam/attacker address as canary
2. Or update canary after deploy (owner only, via `set_canary`)
3. If canary is not set, this layer is inactive —
   velocity + spend cap still protect the wallet

Recommended: set canary to any address from known phishing
lists in your threat model.

## Known Limitations

**Task ID squatting:** Any account can call create_wallet
and claim any task_id. There is no allowlist for who may
create tasks. In production, use unpredictable task IDs
(e.g. UUID or hash-based) to prevent front-running.

**Registry growth:** Completed tasks remain in storage
(marked completed: true). For high-volume deployments,
consider periodic cleanup or a separate registry contract.

## Mainnet Deployment Checklist

Before deploying to mainnet:

- [ ] Delete the full access key from factory account
      after deploy (`near delete-key`)
- [ ] Verify 0 full access keys remain on factory
- [ ] Set canary address from your threat model
- [ ] Test create_wallet + agent_call + force_expire
      end-to-end on testnet first
- [ ] Keep min 2 NEAR balance on factory for storage

> After deleting the full access key, the contract
> is immutable. No upgrade path exists by design.

## Built at NEAR HACKATHON (security track), November 2026

## FUKILLA vs existing solutions

| | OutLayer | AgentLock | FUKILLA |
|---|---|---|---|
| Enforcement | Off-chain server | Off-chain event | On-chain contract |
| Works if server down | ❌ | ❌ | ✅ |
| Self-quarantine | ❌ | ❌ | ✅ |
| Honeypot/canary | ❌ | ❌ | ✅ |
| No backend needed | ❌ | ❌ | ✅ |
| Open source | — | — | ✅ |

## Track eligibility

- Track 06 AI Security & Defense: sandboxed agent wallet, spend limits, function-call keys, self-quarantine
- Track 05 AI Swarms: per-agent isolated wallet, multi-agent coordination safe
- Track 07 Agent Economy: agent wallet infrastructure, spend cap per task

## Ecosystem impact

Any NEAR project giving AI agents money can drop in FUKILLA today: one contract call to `create_wallet`, one function-call key to the agent. No backend. No oracle. No new token. Pure NEAR primitives.

This is infrastructure, not an app. Every AI agent wallet on NEAR is a potential user.

## Build & test

```bash
cargo test          # 19 unit tests + 1 integration test
cargo near build    # reproducible WASM
```

## License

MIT

## Live proof (testnet, 2026-11-06)

| Action | Result | TX |
|---|---|---|
| Factory deployed + initialized | ✅ | `82nihL6tcesZzRYwy8UCyCgWKCiF7mjwTVTobUXFETez` / `8w17hffEXS1GBvSi6dDYckF6VJMvxfyFMmGvKcRVeVmx` |
| `create_wallet` end-to-end (1 NEAR, SENTINEL 10 calls/60s) | ✅ `e2e-proof-2.fukilla.testnet` born with 1 FC key, 1 NEAR | `Hfa8RKvQacmArfX2vPw9C5ouWeaaCsdgDtNyBK2Biy1N` |
| Unauthorized `agent_call` (wrong signer) | ✅ **REJECTED** — funds intact, `spent: 0` | rejected tx |
| Wallet upgraded after live finding | min funding 1 NEAR (executor deploy+storage = 0.845 NEAR measured on-chain) | `6QTzdNN9TZERS2VbJAM4bdgobMPcLET1jCaxagq2qX6J` |

Integration notes for builders:
- `create_wallet` needs **500 Tgas** prepaid (create + deploy executor WASM + add key).
- Funding must be **≥ 1 NEAR** (measured storage cost on testnet: 0.845 NEAR).
- The agent key is function-call-only: `0 full access keys and 1 function-call-only access key` (verified via `near account view-account-summary`).
