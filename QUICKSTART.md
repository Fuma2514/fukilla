# QUICKSTART — FUKILLA in 5 minutes (NEAR Testnet)

Run a cap-limited agent wallet end-to-end: create it, spend with the agent key,
watch the on-chain spend cap reject over-cap payments, then recover the funds.

Prereqs: Node.js 18+, [near CLI](https://github.com/near/near-cli-rs) (`cargo install near-cli-rs`),
a NEAR **Testnet** account with ≥ 6 NEAR (faucet: https://near-faucet.io).
This walkthrough uses TESTNET only.

## 1. Build & deploy the factory

```bash
cargo near build non-reproducible-wasm --locked
# outputs target/near/fukilla.wasm
near deploy <your-account>.testnet wasm target/near/fukilla.wasm
near call <your-account>.testnet new '{"owner": "<your-account>.testnet"}' --accountId <your-account>.testnet
```

## 2. Generate the agent Function-Call key

```bash
cd demo && npm install
node gen-agent-key.js $(openssl rand -hex 32)   # -> {"publicKey":"ed25519:...","secretKey":"ed25519:..."}
cd ..
```

The agent key is an FC key scoped to **`agent_call` on the factory only**, with
allowance = spend_cap + 0.5 NEAR — the agent cannot hold anything more.

## 3. Create a task wallet (owner)

```bash
near call <your-account>.testnet create_wallet '{
  "task_id": "my-first-task",
  "agent_key": "ed25519:<agent-public-key>",
  "ttl_sec": 86400,
  "spend_cap": "50000000000000000000000",   // 0.05 NEAR
  "max_calls_per_window": 3,
  "velocity_window_sec": 60
}' --deposit 2.1 --accountId <your-account>.testnet
```

Funding must be ≥ 2.01 NEAR (2 NEAR executor storage + gas headroom + 0.01 task storage).
Task ids are namespaced per owner: a task_id must start with the first 8 hex chars of
SHA-256(owner) — query it first with
`near view <your-account>.testnet expected_task_prefix '{"owner":"<your-account>.testnet"}'`
and use e.g. `<prefix>my-first-task`.
On success the sub-account `<task_id>.<your-account>.testnet` exists, holding the funds,
with the agent FC key installed.

## 4. Act as the agent (agent key signs)

```bash
# import the agent secret key into near CLI (or use a JS near-api-js signer)
near call <your-account>.testnet agent_call '{
  "task_id": "my-first-task",
  "receiver_id": "<receiver>.testnet",
  "amount": "1000000000000000000000"        // 0.001 NEAR
}' --signerKey ed25519:<agent-secret-key>
```

Works — payment settles, `spent` is recorded in factory state.

## 5. Watch the guard rails fire

- **Spend cap:** repeat step 4 past the cap →
  `Smart contract panicked: spend cap exceeded` — no funds move.
- **Velocity breaker:** make `max_calls_per_window` calls within `velocity_window_sec`
  → the wallet self-quarantines (`velocity_quarantine` event) and refuses further calls.
- **Recovery:** as the owner,
  `near call <your-account>.testnet complete_task '{"task_id":"my-first-task"}'`
  → wallet sweeps its **entire balance back to you** and deletes itself.
- **Lock-out:** `revoke_key '{"task_id":"my-first-task"}'` deletes the agent FC key at the
  protocol level; the agent key stops working immediately.

## Verify what you just saw

- Task state: `near view <your-account>.testnet get_task '{"task_id":"my-first-task"}'`
- The agent key really is an FC key:
  `near keys <wallet-account>.testnet` → one FunctionCall key, method `agent_call`
- Full security posture, live-transaction evidence, and the known critical finding (F-1):
  see [SECURITY.md](SECURITY.md).
