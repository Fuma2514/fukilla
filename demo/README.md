# FUKILLA demo — judge quickstart

From a clean clone, 3 commands:

```bash
# 1. Generate an agent keypair (self-contained, no extra deps beyond npm install)
cd demo && npm install
node gen-agent-key.mjs <any-64-hex-char-seed>
# -> prints {publicKey, secretKey}. Use the publicKey when calling create_wallet.

# 2. Run the attack simulation (state fuzzing, works offline against a sandbox task state)
python3 demo/attack_sim.py        # optional, read-only fuzz demo

# 3. Verify the deployed contract reproduces from source
cd .. && ./verify_deployment.sh
# prints full evidence: commit, rustc, cargo-near version, both hashes, RPC response
# exits non-zero on mismatch
```

What to look for as a judge:

- `sdk/sdk_owner_signer.test.ts` — proves the agent can never sign as anything but itself;
  `complete_task` is signed by the configured owner (`ownerAccountId`), not the factory.
- `src/lib.rs` — `agent_call`: spend cap, SENTINEL velocity breaker (violation succeeds
  but pays nothing and locks the wallet), canary honeypot (bait payment → sweep-back +
  permanent lock).
- `executor/src/lib.rs` — `sweep` / `on_swept` recovery: a failed sweep is retryable,
  spending stays blocked until `delete_account` completes.
- `tests/test_basics.rs` — full wallet lifecycle on a real `near-sandbox` chain.
- `SECURITY.md` — threat model, 7 defense layers, honest scope limits.

Requirements: Node ≥ 20, Python 3, Rust 1.93.0 (pinned by rust-toolchain.toml).
No other hidden dependencies.
