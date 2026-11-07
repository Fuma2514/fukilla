# FUKILLA — Security & Threat Model

> "Burn after paying." — the first on-chain self-quarantining agent wallet.

This document is the defense-in-depth record of FUKILLA: what it defends against,
what it deliberately does not defend against, and the audit trail.

## Architecture layers (7)

| # | Layer | Enforced by | Defeats |
|---|-------|-------------|---------|
| 1 | Factory-only wallet creation | Contract `create_wallet` | Rogue pre-provisioned accounts |
| 2 | Function-Call key (1 method) | NEAR protocol access keys | Key theft → no transfer/deploy/delete powers |
| 3 | Protocol allowance (spend cap) | `Allowance::limited` | Unbounded agent spending, even off-contract |
| 4 | Cumulative cap + expiry | Contract state, `overflow-checks=true` | Slow-drip drains, undead wallets |
| 5 | SENTINEL velocity breaker | On-chain self-quarantine | Prompt-injected runaways, burst attacks — violating tx *succeeds* but pays nothing and locks the wallet forever: unerasable on-chain evidence |
| 6 | Canary honeypot | On-chain | Agent paying attacker bait → sweep-back + permanent lock, attacker never learns it was a trap until funds never arrive |
| 7 | Self-destruct sweep | Executor `delete_account` | Post-task key residue — a leaked key opens nothing |

## Threats explicitly in scope

- **Prompt injection → signed payment**: agent is tricked, not the signer; the
  FC key cannot move funds outside `agent_call`, and `agent_call` is bounded by
  layers 3–6.
- **Runaway agent (agentic DoS)**: velocity breaker locks the wallet on-chain.
- **Key exfiltration after task**: wallet no longer exists; key is dead weight.
- **Storage/gas griefing**: velocity params capped (1..=10000 calls, 1s..=30d
  window); task IDs validated to prevent nested-account funding traps.

## Explicitly out of scope

- Owner key compromise (out of threat model: the owner IS the authority).
- Receiver-contract malice (payments are final once sent; use allowlists at
  the orchestration layer).
- Consensus-level attacks (NEAR validator misbehavior).

## Findings (status)

- **Storage griefing (FIXED)**: `create_wallet` now requires the attached
  deposit to cover a 0.01 NEAR per-task-id storage cost (`TASK_STORAGE_COST`)
  **on top of** the 2 NEAR wallet funding minimum (min 2.01 NEAR). The cost is
  retained by the factory (burn) to pay for the permanent registry entry;
  unbounded free task creation is no longer possible. Tests:
  `create_wallet_without_storage_cost_fails` (negative), `storage_cost_burned_on_creation`
  (positive) — src/lib.rs `#[cfg(test)] mod tests`.
- **Sweep recovery (FIXED)**: `finalize()` no longer marks the task
  `completed` synchronously. It sets `sweeping = true` and chains an
  `on_swept` callback: on promise success the task becomes `completed`; on
  failure `sweeping` is cleared and the sweep can be retried via
  `complete_task` / `force_expire` — funds cannot strand in a still-alive
  wallet from a single failed sweep. While `sweeping`, re-finalization and
  `agent_call` are rejected. Tests: `sweep_failure_is_retryable`,
  `sweeping_wallet_rejects_finalize_and_spending`, plus updated
  `owner_can_complete_and_sweep` / `anyone_can_force_expire_after_window` /
  `canary_payment_sweeps_and_locks` — src/lib.rs `#[cfg(test)] mod tests`.
- **WASM reproducibility (OPEN, auditor)**: the on-chain code hash `53yp7kAJf3rsGeD54ZKS1VeQ4C7WwR9HZ1Jtw4393eC9` matches NONE of the attempted build routes (plain pinned cargo at b2990fb/e9642fb → `51YNFS3zsoMa6LhaL6s2PWh32LADbSEQ12qHU6nJd3sD`; cargo-near 0.22.0 → `cb555ca2…`; pinned Docker reproducible image → `A2hxs4HXmDuJEGHsNnWx5p9dqtVJz6foMnGxKRovWR8W`), and plain builds are path-dependent. Deployed-code-to-source attribution is UNVERIFIED — a finding until reproduced byte-for-byte.

## Audit trail

| Date | Scope | Result |
|------|-------|--------|
| 2026-11-06 | near-contract-audit 16-detector sweep (both contracts) | 0 high; 2 medium/low found & fixed (task_id validation, zero-attach `agent_call`) |
| 2026-11-06 | Sentinel red-team (S1–S15) | 13 pass; 2 findings (velocity param caps) fixed same day; +2 negative tests |
| 2026-11-06 | Reproducible builds | `cargo build --target wasm32-unknown-unknown --release`; tests 16 unit + 1 integration PASS |
| 2026-11-07 | External auditor verification (rustc 1.93.0, `cargo test -j 1` at HEAD e9642fb) | 21 unit + 1 sandbox integration PASS; +5 SDK signer tests PASS (finding A fix in e9642fb verified). `verify_deployment.sh` added (builds b2990fb + e9642fb, hashes vs on-chain code_hash). Storage-griefing & sweep-recovery remain OPEN |
| 2026-11-07 | Storage-griefing & sweep-recovery fixes | `create_wallet` requires 0.01 NEAR per-task storage burn (min 2.01 NEAR); `finalize` sets `sweeping` + `on_swept` callback with retry on failure. 25 unit + 1 sandbox integration PASS (`cargo test -j 1`); `cargo clippy --tests` 0 warnings. Both findings FIXED; WASM reproducibility still OPEN |

Commits: `f069a04` (audit fixes) → `7986d5d` (SENTINEL) → `7a45097` (sentinel hardening).

## Reproduce

```bash
cargo test          # 16 unit + 1 integration
cargo build --target wasm32-unknown-unknown --release
```

Live factory (testnet): `fukilla.testnet`
Deploy tx `82nihL6tcesZzRYwy8UCyCgWKCiF7mjwTVTobUXFETez`
Init tx `8w17hffEXS1GBvSi6dDYckF6VJMvxfyFMmGvKcRVeVmx`
