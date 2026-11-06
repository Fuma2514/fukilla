# FUKILLA 🔥

**"Burn after paying."**

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

- Factory: `ephwallet.testnet`
- Demo wallet: `demo-task-1.ephwallet.testnet` (funded with 1 NEAR, FC key capped at 0.05 NEAR)
- Create wallet tx: [`9ZCX8DJ2FPdGFaAmAt6aci9qWi2usdWLtkjMAmBYKugR`](https://testnet.nearblocks.io/tx/9ZCX8DJ2FPdGFaAmAt6aci9qWi2usdWLtkjMAmBYKugR)

### Security properties

1. **Least privilege** — the FC key can call exactly one method. Nothing else.
2. **On-chain spend cap** — allowance is enforced by the NEAR protocol itself, not by the agent's own discipline
3. **No ownership** — the agent key cannot transfer the account, deploy contracts, or change keys
4. **Self-destruct** — `complete_task` returns all remaining balance to the owner and deletes the account, so leaked keys become worthless
5. **Zero trust** — the owner never shares their main key with anyone, ever

## Built at NEAR HACKATHON (security track), November 2026

## Build & test

```bash
cargo test          # 9 unit tests + 1 integration test
cargo near build    # reproducible WASM
```

## License

MIT
