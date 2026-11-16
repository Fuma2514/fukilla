# FUKILLA 🔥

**"Burn after paying."**

*"Burn after paying" — the wallet self-destructs after the task ends; all remaining funds return to the owner. Nothing is lost.*

*FUKILLA — A self-enforcing, serverless security layer for autonomous AI agent funds.*

Every AI agent that touches crypto has the same fatal design flaw: it holds a full private key to your funds. One leaked key, one compromised instruction, one hallucination — everything gone. FUKILLA prevents compromised AI agents from exceeding predefined on-chain security policies:

- The **owner** funds a fresh, disposable wallet per task ("a Fukilla") through the factory
- The **agent** receives exactly one **Function-Call access key**, scoped to a single method (`agent_call`), with a hard on-chain allowance (spend cap)
- The agent can work and pay — it can never own anything, never drain, never touch the owner's main account
- When the task completes, the balance returns to the owner and the wallet **deletes itself**

> Every transaction is its last.

## Architecture

- **Factory contract** (`src/lib.rs`): `create_wallet` deploys a new sub-account with an embedded executor WASM, registers the task, and adds the agent's FC key with `Allowance::limited`
- **Executor contract** (`executor/`): minimal per-wallet logic — validates `agent_call` against the allowance, settles to the receiver, exposes `complete_task` (sweeps balance to owner, deletes the account) and `force_expire`
- **Demo tooling** (`demo/`): generates the agent FC keypair

---

# VERIFIED EVIDENCE

Facts independently re-verified (external auditor, 2026-11-07) against live on-chain state, real toolchain runs, or actual code execution.

## Toolchain & build

- Rust pinned via `rust-toolchain.toml`: channel `1.93.0`, target `wasm32-unknown-unknown`
- Verified: `rustc 1.93.0 (254b59607 2026-01-19)`, `cargo 1.93.0 (083ac5135 2025-12-15)` (rustup 1.29.3); `cargo-near 0.22.0` (latest supporting rustc 1.93; used by the sandbox integration test)
- Tests run with `cargo test -j 1` on the pinned toolchain
- Deployment verification: `./verify_deployment.sh` — builds the contract at BOTH audit commits (`b2990fb` and HEAD `e9642fb`) in detached git worktrees with the pinned toolchain, hashes each WASM (SHA-256 hex + Base58), queries the on-chain `code_hash` via RPC, reports which commit matches, and **exits non-zero if neither matches**. It prints the full evidence summary (source commit, rustc version, cargo-near version, build command, both hashes, RPC endpoint, raw RPC response). Run it in this repo root to reproduce.

## Release candidate (2026-11-13, commit `5b4f600` on `fix/demo-portability`)

- Build (Docker `sourcescan/cargo-near:0.22.0-rust-1.97.1`, source mounted at `/tmp/bld`,
  `cargo near build non-reproducible-wasm --locked`): WASM **286,754 B**, SHA-256
  **`c1a14a8393979e027441b8fdc94106f20cbc5b20bd12aeb020fdfff128fb4a2f`**,
  Base58 **`E2rLGtHipTcnpi7tVVz2nRTGKJ8pKhEaAgwJcXrFcXTU`**
- **Reproducibility verified**: two independent Docker builds from host paths of differing
  lengths produced byte-identical artifacts (verbatim build logs:
  `release_archive/e2e_evidence/build_logs/dockerA.log` / `dockerB.log` — each ends with
  cargo-near's own identical checksum report)
- Tests at `5b4f600`: unit **35/35**, sandbox **2/2** (incl. a real sandbox-chain F2b
  revocation test: delete receipt success, agent key dead post-revoke, recovery intact)
- **Live-verified on NEAR Testnet** (isolated E2E account `fukilla-e2e-11130138.testnet`,
  deployed code_hash = Base58 of the artifact SHA-256 — exact match): live revocation
  (`revoke_key` tx `2RjoCAjYE9AbnjpQ2Z1gc4xBrqGYSiPuWFZBVm3FovGU`, key verifiably dead via
  `view_access_key`, post-revoke signing rejected, task recovered), over-cap panic with
  funds untouched, velocity self-quarantine, and the full canary-honeypot sequence
  (`set_canary` → trigger → wallet destroyed → owner recovered 2.2312 NEAR → bait balance
  0 → 0). Every claim carries a transaction hash or captured RPC error — see
  [SECURITY.md](SECURITY.md) §2. One disclosed gap: individual velocity-burst tx hashes
  were not captured; the quarantine state was verified via RPC.
- The public `fukilla.testnet` account still runs the older bytecode and was NOT touched
  during this RC's testing.

## Verified redeployment (2026-11-15, master `b2549ce`)

- The current master source builds **byte-identically** to the RC artifact:
  `cargo near build non-reproducible-wasm --locked` (rustc 1.97.1,
  cargo-near 0.22.0) → WASM **286,754 B**, SHA-256
  **`c1a14a8393979e027441b8fdc94106f20cbc5b20bd12aeb020fdfff128fb4a2f`**,
  Base58 **`E2rLGtHipTcnpi7tVVz2nRTGKJ8pKhEaAgwJcXrFcXTU`** — the same hash
  as the RC artifact and the E2E deployment. (Build-path note: the WASM
  embeds dependency file paths, so reproduction requires
  `CARGO_HOME=/home/near/.cargo`, matching the Docker image; a different
  `CARGO_HOME` shifts the hash, e.g. `2ea970a8…` for a workspace cargo home.)
- Fresh deployment on NEAR Testnet: **`fukilla-v2.testnet`** — deploy tx
  `39UB6fXj4HS1FUxSuTp2anZSXsDTWjjwsXFrihYccKJS`; on-chain `code_hash`
  `E2rLGtHipTcnpi7tVVz2nRTGKJ8pKhEaAgwJcXrFcXTU` (**exact match**), confirmed
  at the bytecode level (downloaded deployed code hashes to `c1a14a83…`).
- End-to-end on the new deployment: init tx
  `53z8KkFtZgFvUGWFTKjvmmgUskX9iFeXez1G9vpHHyrQ`; `create_wallet` tx
  `5Y6AVQf1QyZSRZh9B316EZyKFDu2prpZNaXsYwyDUk5s` → wallet
  `8f6f3ef4verify-01.fukilla-v2.testnet` (2.1 NEAR, 1 FunctionCall key);
  agent payment tx `DbdjJekM4ac9GactAU7K66qjm32LAMS5ZZfGXSQmLVVu`
  (0.001 NEAR, `spent` recorded on-chain).
- The original `fukilla.testnet` account still runs the older bytecode
  (`code_hash` `53yp7kAJf3rsGeD54ZKS1VeQ4C7WwR9HZ1Jtw4393eC9`, 311,400 B)
  and was not touched; it remains the deployment shown in the demo video.

## On-chain (testnet, verified via https://rpc.testnet.near.org)

- Factory account: `fukilla.testnet` (signer & receiver of deploy tx `GEwkF97Q1gdu6jHRu7hZCNpAsXhqAqunwZD69YtSbs2w`, action `DeployContract`, status `SuccessValue` — verified by fetching the tx via RPC)
- On-chain `code_hash` of `fukilla.testnet` (queried at audit time): `53yp7kAJf3rsGeD54ZKS1VeQ4C7WwR9HZ1Jtw4393eC9` (storage usage 312287 bytes)
- **Latest deployment transaction: `6QTzdNN9TZERS2VbJAM4bdgobMPcLET1jCaxagq2qX6J`** (confirmed via archival RPC — the most recent `DeployContract` on `fukilla.testnet`). Earlier tx `GEwkF97Q1gdu6jHRu7hZCNpAsXhqAqunwZD69YtSbs2w` is a historical deployment, superseded.
- Deployment hash verification (2026-11-07, `./verify_deployment.sh`): **MISMATCH — none of the local builds reproduce the on-chain code hash.** All three build routes produce distinct WASMs:
  - plain pinned `cargo build -j 1 --locked --release --target wasm32-unknown-unknown` at `b2990fb` and at `e9642fb` (factory code identical between them): SHA-256 `3b9236b90523da59e1549c2d9a5af89f30acc201c2a7dd7ae7fb2b60cecf5d38` = Base58 `51YNFS3zsoMa6LhaL6s2PWh32LADbSEQ12qHU6nJd3sD` (built in /tmp worktrees; the same commit built in the repo directory yields a DIFFERENT hash `63cc3db49abd634890e10caa2cb56830e43edf94fe8695258267ea1cec0b7bfb` — local plain builds are path-dependent)
  - `cargo near build non-reproducible-wasm --locked` (cargo-near 0.22.0, rustc 1.93.0): SHA-256 `cb555ca2d01b32d764cd2abdce0c9c3d6bc1a459340740c322cafc1b17318d07`
  - **Re-verified 2026-11-12 (cargo-near 0.22.0, /tmp worktree at `b2990fb`):** SHA-256 `63cc3db49abd634890e10caa2cb56830e43edf94fe8695258267ea1cec0b7bfb`, size **311,168 B** — closest to the deployed WASM (311,400 B, delta = 232 bytes, first divergence at byte 2864, consistent with a shifted data-section offset). One commit earlier, `bf8f060` (cargo-near route): 304,496 B — excluded. `b2990fb`-era source is confirmed as the deploy-time route, but the exact source state remains **not reconstructible from git** (likely an uncommitted working tree at deploy time).
  - `cargo near build reproducible-wasm` with the pinned Docker image (`sourcescan/cargo-near:0.21.1-rust-1.96.0@sha256:ccb22bb4…`): SHA-256 `86291490cab62c8e08a1237e3f55d0d2b59481df5e0504a918e69e46d086aeab` = Base58 `A2hxs4HXmDuJEGHsNnWx5p9dqtVJz6foMnGxKRovWR8W`
  - On-chain: `53yp7kAJf3rsGeD54ZKS1VeQ4C7WwR9HZ1Jtw4393eC9`. **This is an open audit finding: the deployed contract cannot be reproduced byte-for-byte by any route attempted** (possible causes: different commit/toolchain at deploy time, or a non-pinned build; tx `GEwkF97Q1gdu6jHRu7hZCNpAsXhqAqunwZD69YtSbs2w` only carries ~37 bytes of code metadata, so the deployed source cannot be confirmed from the tx alone). Treat deployed-code-to-source attribution as UNVERIFIED.
- Factory deploy + init txs: `82nihL6tcesZzRYwy8UCyCgWKCiF7mjwTVTobUXFETez` / `8w17hffEXS1GBvSi6dDYckF6VJMvxfyFMmGvKcRVeVmx`
- `create_wallet` end-to-end: `Hfa8RKvQacmArfX2vPw9C5ouWeaaCsdgDtNyBK2Biy1N`
- Min-funding upgrade tx: `6QTzdNN9TZERS2VbJAM4bdgobMPcLET1jCaxagq2qX6J`

## Test results (2026-11-07, rustc 1.93.0, `cargo test -j 1` at HEAD `e9642fb`)

- Unit tests (`src/lib.rs`): **21 passed, 0 failed**; re-run 2026-11-12 at `1283c06`: **25 passed, 0 failed** (includes storage-cost burn, sweep-recovery `on_swept` callback, velocity pairing, and memo-injection hardening tests)
- Sandbox integration test (`tests/test_basics.rs`, real `near-sandbox` chain + `cargo near` build): **1 passed, 0 failed**
- SDK signer tests (`sdk/sdk_owner_signer.test.ts`, `node --experimental-strip-types --test`): **5 passed, 0 failed** — proves the SDK signs `create_wallet` AND `complete_task` with the configured `ownerAccountId` (e.g. `alice.owner.testnet`), not the factory account (GPT audit finding A, fixed in `e9642fb`, verified here by test).

---

# IMPLEMENTATION CLAIMS

Statements the code implements but that are NOT independently verified against live chain state; taken on the code's face. Verify with `verify_deployment.sh` and your own tx inspection before trusting them.

1. **Least privilege** — the FC key can call exactly one method (`agent_call` on the factory) — enforced by `Allowance::limited(..., env::current_account_id(), "agent_call")` in `create_wallet` (`src/lib.rs`).
2. **On-chain spend cap** — `spent <= spend_cap` enforced in `agent_call` (`require!(new_spent <= t.spend_cap)`); the protocol allowance adds a fixed gas margin (`GAS_ALLOWANCE_MARGIN`) and is NOT the real cap. **Correction (2026-11-15, finding F-A):** the margin is **500 NEAR**, not 0.5 NEAR as originally documented — see [SECURITY.md](SECURITY.md) §3.
3. **No ownership** — the agent key cannot transfer the account, deploy contracts, or change keys (properties of FC keys + `signer == task.wallet` gate).
4. **Self-destruct** — `complete_task` / `force_expire` → `finalize` → executor `sweep` → `delete_account(beneficiary)` (`src/lib.rs::finalize`, `executor/src/lib.rs::sweep`).
5. **Isolation of funds** — `agent_call` forwards to the executor's `pay`, which transfers from the sub-account's own balance (`executor/src/lib.rs::pay`).
6. **SENTINEL velocity breaker / canary quarantine** — violating calls succeed but pay nothing and set `quarantined`/`completed` (`agent_call` velocity branch, canary branch → `finalize`).
7. **Refund correctness** — `on_wallet_created` refunds the stored `funding` on promise failure (not `attached_deposit()`, which is 0 in callbacks).
8. **Historical live-proof table below** — the tx table and attack-simulation results are the team's own reports; the factory account and its deploy tx are independently verified (above), the per-action outcomes are not.

Historical live-proof table (team-reported, 2026-11-06):

| Action | Result | TX |
|---|---|---|
| Factory deployed + initialized | ✅ | `82nihL6tcesZzRYwy8UCyCgWKCiF7mjwTVTobUXFETez` / `8w17hffEXS1GBvSi6dDYckF6VJMvxfyFMmGvKcRVeVmx` |
| `create_wallet` end-to-end (1 NEAR, SENTINEL 10 calls/60s) | ✅ `e2e-proof-2.fukilla.testnet` born with 1 FC key, 1 NEAR | `Hfa8RKvQacmArfX2vPw9C5ouWeaaCsdgDtNyBK2Biy1N` |
| Unauthorized `agent_call` (wrong signer) | ✅ **REJECTED** — funds intact, `spent: 0` | rejected tx |
| Wallet upgraded after live finding | min funding raised (executor deploy+storage = 0.845 NEAR measured on-chain) | `6QTzdNN9TZERS2VbJAM4bdgobMPcLET1jCaxagq2qX6J` |

---

# Gas accounting / economic exposure

Maximum economic exposure per wallet, derived from `src/lib.rs` and `executor/src/lib.rs`:

| Component | Bound | Code path |
|---|---|---|
| Storage staking (executor WASM + account + key) | Measured **0.845 NEAR** on testnet; code requires funding **≥ 2 NEAR** (`src/lib.rs::create_wallet`, `require!(funding >= 2e24)`) | executor WASM embedded via `include_bytes!("../executor.wasm")`; deployed with `Promise::new(wallet).create_account().transfer(funding).deploy_contract(...)` |
| Agent outflow (payments) | Hard-capped at `spend_cap` (cumulative): `require!(new_spent <= t.spend_cap)` in `agent_call`; enforced **before** the `pay` promise | `src/lib.rs::agent_call` |
| Agent gas via protocol allowance | FC key allowance = `spend_cap + GAS_ALLOWANCE_MARGIN` — burnt gas is deducted from this allowance, so gas can be consumed by the agent key *without* a successful payment and WITHOUT incrementing `spent` (finding D: gas budget is separate from the payment cap). **Correction (F-A, 2026-11-15): the margin is 500 NEAR, not 0.5 NEAR** — the gas-burn bound is min(500 NEAR, wallet balance), not 0.5 NEAR; see [SECURITY.md](SECURITY.md) §3 | `create_wallet`: `Allowance::limited(cap + GAS_ALLOWANCE_MARGIN)`; worst realistic case ~300 Tgas × 1e10 = 3 NEAR at peak gas price is covered by the 500 NEAR margin (bounded loss = allowance size) |
| `agent_call` execution gas | Charged to the caller (the wallet sub-account via the FC key) — 5 Tgas cross-contract `pay` call | `agent_call`: `Gas::from_tgas(5)` |
| Sweep/teardown gas | 20 Tgas `sweep` call + `delete_account` | `finalize`: `Gas::from_tgas(20)` |
| `create_wallet` gas | ~500 Tgas prepaid, paid by the creating owner, not the wallet | README integration notes + promise chain size |

**Maximum economic exposure per wallet** = `funding (≥ 2 NEAR, owner-supplied) + gas burnt against the FC-key allowance`. **Correction (F-A, 2026-11-15): the allowance margin is 500 NEAR, not 0.5 NEAR**, so the gas-burn bound is min(500 NEAR, wallet balance) — for a typically funded wallet the agent could burn most of its liquid balance in gas, not just 0.5 NEAR; the previously stated ≈ **2.5 NEAR** total assumed the 0.5 NEAR figure and is superseded — see [SECURITY.md](SECURITY.md) §3. Of the funding, up to `spend_cap` can reach third parties via `agent_call`; the remainder (funding − spend_cap − agent gas burnt) is recoverable by the sweep (`sweep` / `delete_account`), except in the sweep-failure scenario below. The factory account itself is exposed only to its own gas and the storage-griefing finding (registry entries are paid by creators' deposits, but completed entries are never cleaned).

**What cannot be derived from code** (missing for exact numbers):
- No constant for the executor WASM byte size is asserted in code — the 0.845 NEAR storage figure is a one-time measurement, not checked at build time.
- Gas prices are dynamic (`1e9–1e10 yoctoNEAR/gas` per code comments); exact worst-case gas NEAR value cannot be fixed at compile time. **Correction (F-A, 2026-11-15):** the granted allowance is `cap + 500 NEAR`, not `cap + 0.5 NEAR` — a 300 Tgas peak-price call (~3 NEAR) is covered by the margin, so the agent tx would NOT fail at peak prices; the earlier "funds safe, task may stall" conclusion is superseded — see [SECURITY.md](SECURITY.md) §3.
- Sweep-failure refund accounting: `finalize` marks `completed = true` *before* the sweep promise resolves and has no callback/retry; the exact recoverable balance on sweep failure depends on runtime state not modeled in code (see DOCUMENTED LIMITATIONS).

---

# DOCUMENTED LIMITATIONS (OPEN findings)

**Task ID squatting** (FIXED in HEAD): `create_wallet` now requires the task_id to start with the first 8 hex chars of SHA-256(owner) — a per-owner namespace enforced on-chain (`expected_task_prefix` view for SDKs/demos). Squatting another owner's ids is infeasible; front-running only works against an *exactly* published id. Residual: use unpredictable suffixes (e.g. UUID) for unannounced tasks.

**Registry growth** (OPEN): Completed tasks remain in storage (marked completed: true). For high-volume deployments, consider periodic cleanup or a separate registry contract.

**Storage griefing** — **STATUS: OPEN (not fixed, not tested)**: Anyone can create tasks. High-volume task creation inflates factory storage. Mitigate with unpredictable task IDs and off-chain rate limiting. This finding remains open until a fix (e.g. owner-signed creation or fee/burn per task id) is implemented AND covered by a passing test.

**Agent-key revocation** (FIXED in HEAD, `revoke_key`): the task owner can immediately delete the agent's function-call key from the wallet (`revoke_key(task_id)`, owner-gated). After revocation the agent cannot sign anything — spending stops at the protocol level, not just at the contract level. The task remains recoverable via `complete_task`/`force_expire` (sweep-back does not depend on the agent key). Covered by unit tests (`revoke_key_wrong_caller_panics`, `revoke_key_by_owner_emits_delete_key_promise`, `revoke_key_unknown_task_panics`).

**Sweep recovery** — STATUS: FIXED (tested): `finalize` sets `sweeping = true` and the `on_swept` callback clears it; a failed sweep does NOT mark the task completed and stays retryable via `force_expire`. Regression test: "a failed sweep must not mark the task completed" (unit, green at HEAD).

**WASM reproducibility** (OPEN): `verify_deployment.sh` compares locally built WASM hashes against the on-chain `code_hash`. If they differ (path/rustc nondeterminism, cargo-near Docker builds vs. plain builds), that is reported as an explicit MISMATCH with both hashes — treat the deployed code as unverified until reproduced byte-for-byte (see `Cargo.toml` `[package.metadata.near.reproducible_build]` for the Docker route).

**Gas budget is separate from the payment cap** (finding D, kept as a limitation):
The agent's function-call key allowance is
`spend_cap + GAS_ALLOWANCE_MARGIN`. Transaction
gas consumed by the agent is paid from that allowance and
does NOT increment `spent`. A compromised agent could burn
gas (e.g. repeated zero-value payments) without ever
touching the payment cap — total economic loss is bounded
by `spend_cap + gas budget`, not `spend_cap` alone.
**Correction (F-A, 2026-11-15): the margin is 500 NEAR, not 0.5 NEAR**
as originally stated here — the gas budget is min(500 NEAR, wallet
balance). If you need to cap the entire budget, lower the key
allowance or treat the margin as an additional risk budget.
See [SECURITY.md](SECURITY.md) §3.

---

## Bugs found & fixed during development

We publish these because a security project that claims to be flawless is the least credible one on the table.

1. **Payments came from the factory, not the task wallet.** In `agent_call`, the transfer was executed by the factory contract — meaning all tasks shared one pool of funds, contradicting the "isolated per-task wallet" claim. **Fix (commit `13a067d`):** the factory now forwards a `pay(receiver, amount)` call to the executor sub-account, which transfers from its own balance after verifying the caller is its parent factory. Isolation is now real at the protocol level.
2. **Refunds sent 0 NEAR.** The failure callback refunded `env::attached_deposit()` — which is always **0 inside a NEAR callback**, so a failed `create_wallet` silently kept the owner's deposit. **Fix (commit `13a067d`):** the attached deposit is stored in task state (`funding`) *before* the promise chain starts, and the callback refunds from that stored value.
3. **Minimum funding was too low (found live).** A 0.05 NEAR funding failed on-chain: deploying the executor WASM + storage staking costs a measured **0.845 NEAR**, and with only 1 NEAR the wallet's liquid balance after staking was too thin for the agent's gas. **Fix (commits `9996a16`, `13a067d`):** minimum funding raised to **2 NEAR**, proven by the sandbox integration test.
4. **Debugging lesson:** a `env::account_balance()` comparison inside the executor caused a silent wasm trap (panic=abort swallows messages); it was removed — the runtime already fails a transfer exceeding the wallet's balance, and the factory's spend cap bounds total outflow.

## Quickstart

### Prerequisites
- Rust + cargo (pinned: `rust-toolchain.toml` → 1.93.0, installed via rustup)
- NEAR CLI / cargo-near (integration test uses `cargo near`)

### Run tests
```bash
cargo test -j 1 -- --nocapture
```

### Run clippy
```bash
# Note: near-sdk v5 requires wasm32 target for clippy
cargo clippy --lib -j 1 --target wasm32-unknown-unknown -- -D warnings
```

### Deploy to testnet
```bash
cargo near build && near deploy fukilla.testnet target/wasm32-unknown-unknown/release/fukilla.wasm
```

### Verify deployment (auditor)
```bash
./verify_deployment.sh              # ACCOUNT_ID=fukilla.testnet by default
```

### Run SDK signer tests
```bash
sdk/run_sdk_tests.sh    # 5 tests; uses a local no-network near-api-js stub
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

> Note: Canary is a heuristic defense, not a guarantee.
> An agent with access to the contract source can avoid
> the canary address. Primary protection comes from
> spend cap and velocity breaker.

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

## FUKILLA vs existing solutions

| | OutLayer | AgentLock | FUKILLA |
|---|---|---|---|
| Enforcement | Off-chain server | Off-chain event | On-chain contract |
| Works if server down | ❌ | ❌ | ✅ |
| Self-quarantine | ❌ | ❌ | ✅ |
| Honeypot/canary | ❌ | ❌ | ✅ |
| No backend needed | ❌ | ❌ | ✅ |
| Open source | — | — | ✅ |

## Security

Full security posture, threat model, and audit evidence: **[SECURITY.md](SECURITY.md)**.
Highlights: live-verified revocation/quarantine/canary controls; internal security review
(Performed by Hermes, not an independent external audit) with finding **F-1 (factory
full-access key) classified CRITICAL and unmitigated in this RC**; a proposed (not
implemented) multisig + 48h-timelock upgrade roadmap; no formal verification; no
production-readiness claim. 5-minute testnet walkthrough: **[QUICKSTART.md](QUICKSTART.md)**.

## Track eligibility

- Track 06 AI Security & Defense: sandboxed agent wallet, spend limits, function-call keys, self-quarantine
- Track 05 AI Swarms: per-agent isolated wallet, multi-agent coordination safe
- Track 07 Agent Economy: agent wallet infrastructure, spend cap per task

## Ecosystem impact

Any NEAR project giving AI agents money can drop in FUKILLA today: one contract call to `create_wallet`, one function-call key to the agent. No backend. No oracle. No new token. Pure NEAR primitives.

This is infrastructure, not an app. Every AI agent wallet on NEAR is a potential user.

Integration notes for builders:
- `create_wallet` needs **500 Tgas** prepaid (create + deploy executor WASM + add key).
- Funding must be **≥ 2 NEAR** (measured storage cost on testnet: 0.845 NEAR).
- The agent key is function-call-only: `0 full access keys and 1 function-call-only access key` (verified via `near account view-account-summary`).

---

## NEAR HACKATHON 2026 submission

- **Event:** NEAR HACKATHON (2026-11-01 → 2026-11-14, https://nearhackathon.com)
- **Track:** `security` — AI-controlled money made safer (spend caps, sandboxed per-task wallets, function-call keys)
- **Status at this repo:** all code below HEAD is written inside the hackathon window; earlier commits (`b2990fb`, `100f29b`) are the pre-hackathon baseline.
- **Judge quickstart:** clone → `cargo test -j 1` (35 unit tests + 2 near-sandbox integration
  tests at HEAD) → `./verify_deployment.sh` reproduces the audit evidence against testnet
  `fukilla.testnet`. Demo tooling in `demo/`; integration guide in `SUBMISSION.md`;
  security evidence in `SECURITY.md`; 5-minute walkthrough in `QUICKSTART.md`.

## License

MIT

## Built at NEAR HACKATHON (security track), November 2026
