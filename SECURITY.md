# FUKILLA — SECURITY.md

**Release candidate under audit:** commit `5b4f600541e68e35a8480329cdc3934b9ca0818d`
**Artifact:** `rc_5b4f600_docker.wasm` — SHA-256 `c1a14a8393979e027441b8fdc94106f20cbc5b20bd12aeb020fdfff128fb4a2f`
(reproducible build, byte-identical across two independent Docker builds — evidence in §2)

This document describes **implemented and live-verified controls**, **known vulnerabilities**,
and a **proposed (NOT implemented, NOT tested) roadmap**. It is written to be checked against
evidence: every on-chain claim carries a transaction hash or, for calls rejected on-chain,
the captured RPC execution error (a rejected call produces an error result, not a successful
transaction). One gap is disclosed explicitly: individual transaction hashes for the
velocity-burst calls in the live test were not captured; the resulting quarantine state was
verified through RPC. Every code claim carries a source-code line reference verified against
the exact RC commit.

---

## 1. Architecture and enforcement mechanisms (implemented in RC 5b4f600)

FUKILLA is a NEAR Protocol smart-contract system for time-boxed, cap-limited agent wallets:
a **factory** (`src/lib.rs`) creates ephemeral **wallet sub-accounts** running a minimal
**executor** (`executor/src/lib.rs`), each funded for one task and governed by on-chain policy.

| Layer | Mechanism | Enforcement point (verified at 5b4f600) |
|---|---|---|
| Protocol confinement | The agent key is a FunctionCall key restricted to method `agent_call` on the factory, with allowance = spend_cap + GAS_ALLOWANCE_MARGIN | NEAR access-key permission model (`create_wallet` L103, `add_access_key_allowance` L218–224). **Correction (F-A): the margin is 500 NEAR, not 0.5 NEAR as originally stated** |
| Direct-signer requirement | `agent_call` requires `signer == predecessor` (no cross-contract relay) | src/lib.rs L288 |
| No stray deposits | `agent_call` panics on any attached deposit | src/lib.rs L294 |
| Ownership binding | signer must equal the task wallet | src/lib.rs L300 |
| Lifecycle gates | rejects completed / sweeping / quarantined / expired wallets | src/lib.rs L303–308 |
| Velocity breaker | > N calls in window → wallet self-quarantines; the tripping call pays NOTHING | src/lib.rs L310–337 (`velocity_quarantine` log emitted at L332) |
| Spend cap | `spent + amount <= spend_cap` checked in state before any payment | src/lib.rs L343 |
| Canary honeypot | payment targeting the owner-set canary locks the wallet, sweeps everything to the task owner; **no funds ever reach the bait** | src/lib.rs L349–363 |
| Quarantine gate | quarantined wallets accept no agent calls; `lift_quarantine` is task-owner-only | src/lib.rs L303–308; `lift_quarantine` L400 |
| Key revocation (F2b) | `revoke_key` (task-owner-only) commands the wallet's own executor to `delete_key` the agent FC key — protocol-level lock-out | `revoke_key` L419–443; executor `revoke_agent_key` L62–73 |
| Fund recovery | `complete_task` (owner, L448) / `force_expire` (anyone, after expiry, L459) → executor `sweep` deletes the wallet and sends the **entire balance to the task owner only**; failed sweeps are retryable via the `#[private] on_swept` callback (L513–514) | src/lib.rs L448–534; executor `sweep` L18–35 |
| Executor gating | all three executor methods (`pay` L40, `sweep` L18, `revoke_agent_key` L62) require `predecessor == factory` | executor/src/lib.rs L20, L41, L63 |
| Creation hygiene | min funding 2.01 NEAR incl. task storage cost (L122, L188); ttl ≤ 30 days (L125); velocity params must be paired (L136); task-id namespace from owner; duplicate-id rejection | src/lib.rs L103–192 |
| Failure refund | wallet-creation promise failure → refund owner + drop registry entry (`on_wallet_created`, L251, `#[private]`) | src/lib.rs L251–277 |

All source-code line references above were verified against
`git show 5b4f600:src/lib.rs` and `git show 5b4f600:executor/src/lib.rs`.

There is deliberately **no upgrade method, no migration, no admin path** in the RC beyond the
owner-gated operations above. The factory account's full-access key is the one protocol-level
power outside the contract — see F-1.

## 2. Test coverage and live evidence (RC 5b4f600)

**Automated:** 35 unit tests (src/lib.rs) + 2 sandbox tests (tests/test_basics.rs) — all green
at commit 5b4f600, including: velocity identical-timestamp burst, restore-after-quarantine,
completed-task force_expire race guard, task recoverability after revoke_key, wallet-creation
failure refund, 1-yocto dust payment exactness, dust wallet full lifecycle, and a real
sandbox-chain test proving the F2b revocation (delete receipt success, agent key dead,
recovery intact).

**Reproducible build evidence:**
- Command (inside both builds): `cargo near build non-reproducible-wasm --locked`,
  executed in Docker image `sourcescan/cargo-near:0.22.0-rust-1.97.1` with the source
  mounted at `/tmp/bld`.
- Two independent builds were run from host paths of differing lengths (the known
  path-dependence variable). Both complete build logs are preserved verbatim in the release
  evidence archive: `release_archive/e2e_evidence/build_logs/dockerA.log` and
  `release_archive/e2e_evidence/build_logs/dockerB.log` (6,695 bytes each; SHA-256
  `e4a57390a656e7cb955caff6a50a37282f43f7af432327926f0f35a1261a46a2` and
  `a20f74a87c3e7ab3d02ec5ad9a0b84421d5286ba67a2bbb210c98e0d8c3e4f72`).
- Each log ends with cargo-near's own checksum report: SHA-256 hex `c1a14a8393979e02…fb4a2f`,
  bs58 `E2rLGtHipTcnpi7tVVz2nRTGKJ8pKhEaAgwJcXrFcXTU` — **identical in both logs**, proving
  the two builds are byte-identical despite interleaved compile-line ordering differences.
- One canonical artifact is archived at
  `/root/makefun/release_archive/rc_5b4f600_docker.wasm` (286,754 bytes, SHA-256
  `c1a14a83…fb4a2f` re-verified). The second build's artifact was not separately retained;
  its byte-identity is established by the checksum printed in `dockerB.log`.
- Host builds outside Docker are path-dependent (panic-location strings embedded in the
  binary) and are disclosed as non-canonical.

**Live NEAR Testnet evidence (isolated E2E account `fukilla-e2e-11130138.testnet` ONLY):**
Raw results preserved in `/root/e2e_phase2_results.json` and the canary results JSON.

| Property | Transaction / Result |
|---|---|
| Deployed RC code hash | `E2rLGtHipTcnpi7tVVz2nRTGKJ8pKhEaAgwJcXrFcXTU` = base58 of the artifact SHA-256 (RPC `view_account`) |
| Over-cap rejection (task `b834d0cfadv321`, created via `create_wallet` tx `A8W7oEsqgZtkvkLc8x3ce4nNwUL7maSeurxrVDruhQwb`) | rejected call has **no successful tx** — captured RPC error: `Smart contract panicked: spend cap exceeded`, `spent_after=0`, `quarantined=false` (clean reject, no funds moved) |
| Velocity trip (same task) | burst of 3 calls/60s → `velocity_quarantine` log, `quarantined=true` (individual burst-call hashes not captured; the state transition is verified via RPC `get_task`) |
| Legit call (same task) | tx `EPc4DXGiARcCwG7L75yPtL3n5FxYk5rqMjRPxgUDziWb` |
| Canary honeypot (full sequence) | set_canary `CqtSpvp137mDKnjjdhwzmMKFEg2Re4yNkQUjgFr7BwJ6`; create `J7y8yEKV5MDmZnd5PbW4pde2hdjGZ1GVwaKhjZQWDgoG`; trigger `8iv3B7U1BmFo8o2NFvexNPMTkT6ukiqzTtNmQJieYsLd` (`canary_triggered` + `sweep_initiated`); wallet destroyed on-chain; owner recovered 2.2312 NEAR; **bait account balance 0 → 0** |
| Revocation (F2b, live) | `revoke_key` `2RjoCAjYE9AbnjpQ2Z1gc4xBrqGYSiPuWFZBVm3FovGU` — delete receipt success, log `agent_key_deleted`; RPC `view_access_key` → "access key … does not exist while viewing"; post-revoke signing rejected ("doesn't have access key"); task recovered via `complete_task` (wallet `UNKNOWN_ACCOUNT` after) |
| Recovery sweep | sweep→destroy logs on `5CMqD3P39LokkBPrWQUJ9MrnjMjMcHYtn3dSYzQCBV8u` (`sweep_initiated`→`wallet_sweep`→`wallet_destroyed`) |
| Second revocation live | revoke tx `2g19yDnvDew3K1oE5rbXuEJsujzrWHMMq1dMhBjCf5Jy`; lift_quarantine `BmccCmHJpFTzffKvwdJVCzdV4wKseZxT4b4hAvtmLUMx`; complete_task `Cqi5FHrxMxLjdy7BjSBTvyFWUSvXh3G3HMc5xH6Bb9Vv` |

Canary classification is **PASS for the tested scenario only** (owner-set bait account,
agent-initiated payment). Other bait configurations were not tested.
Only the **individual velocity-burst transaction hashes** were not captured; the resulting
quarantine state (`quarantined=true`) was verified through RPC. This is classified as an
**evidence gap**, not a PASS.

All transactions above are on NEAR **Testnet**. No mainnet interaction has ever occurred.

## 3. Internal security review — findings

The review was performed **directly by Hermes** (the developing agent), re-reading the
immutable snapshot (`fukilla-5b4f600-snapshot.tar.gz`, SHA-256
`7fffa8c5e23cbff67073883b037f997345c15c6e7b0cdae61cdebb7d9db70f5b`).
**It is not an independent external audit**, and its conclusions have not been
third-party verified. Full report: `SECURITY_REVIEW_CONSOLIDATED.md`.

- **F-1 — CRITICAL, UNMITIGATED in RC 5b4f600.** The factory account is controlled by a
  single full-access key. A holder of that key can deploy arbitrary new factory contract
  code (NEAR protocol-level action), which could redirect future sweep flows and spend the
  factory's own balance. Mitigation (multisig/timelock) is **proposed, not implemented**.
- F-2 (LOW, by design): the call that fills the velocity window still executes before the
  breaker trips; total outflow remains bounded by the spend cap.
- F-3 (LOW, theoretical): agent key allowance includes 0.5 NEAR gas headroom — bounded
  gas-griefing surface, no fund-theft path. **Superseded by F-A (2026-11-15): the margin
  is actually 500 NEAR, not 0.5 NEAR.**
- F-4 (LOW, theoretical): `agent_call` panics on unknown task ids; caller loses only their
  own gas; no state effect.
- **F-A (OPEN, found 2026-11-15 during the verified redeployment).**
  `GAS_ALLOWANCE_MARGIN` (`src/lib.rs:73`) is 500 NEAR, not 0.5 NEAR — the literal
  `500_000_000_000_000_000_000_000_000` yocto equals 500 NEAR despite the "0.5 NEAR"
  comment and docs. Proven on-chain: the E2E wallet's FC key allowance is 500.05 NEAR
  at creation (0.05 NEAR spend cap + 500 NEAR margin). Effect: the agent key's gas-burn
  griefing bound is min(500 NEAR, wallet balance), not 0.5 NEAR — for a typically
  funded wallet the agent could burn most of its liquid balance in gas. Not a
  fund-theft issue (`spend_cap` is still enforced on-chain). Fixing the constant
  changes the WASM hash and requires a rebuild + redeploy; no fix is applied in this
  RC. This finding supersedes the 0.5 NEAR figures in F-3 above, §1, §6 item 4,
  README, and QUICKSTART.

Verified non-issues (guards present, cited in the consolidated report): canary bypass via
direct executor calls; reentrancy/double-spend on sweep; callback spoofing (`#[private]`
on both callbacks); unauthorized executor access.

## 4. Proposed F-1 mitigation — ROADMAP ONLY (NOT implemented, NOT tested)

Nothing below exists in RC 5b4f600. This is a design feasibility assessment based on
reading upstream source code; **F-1 must be treated as CRITICAL and unmitigated until the
following is implemented, tested, and — critically — until an on-chain key-list check
proves the factory account holds zero full-access keys.**

Proposed architecture (upstream sources cited in §7):
1. Integrate **near-plugins `Upgradable` + `AccessControllable`** into the factory:
   staged code (`up_stage_code`) → deployment (`up_deploy_code`) with hash verification
   and migration-rollback; a **48-hour staging duration** (shortening requires a two-phase
   update that itself waits ≥ the current duration).
2. Authorize up-* roles for a **2-of-3 multisig controller** (NEAR core-contracts
   `multisig2`) whose members are FunctionCall access keys scoped to
   `add_request/confirm` — no full-access keys on the controller.
3. Migrate: one final visible deploy using the existing key → set staging duration →
   grant roles → deploy controller → **delete the factory full-access key** →
   verify via RPC `view_access_key_list` that zero FAKs remain.

Residual risks even after the roadmap: collusion of 2 of 3 signers (visible on-chain,
with a 48-hour opt-out window during which task owners can close wallets); controller
unavailability freezes upgrades (fund-safety unaffected); dependency on pinned upstream
plugin code. We do **not** claim F-1 would be reduced to any specific severity until
implemented and verified.

## 5. Threat model and trust assumptions

- **Trusted:** NEAR protocol validators; the pinned compiler/toolchain (reproducible build
  mitigates toolchain drift).
- **Per-task trust boundary:** the agent key-holder can spend up to `spend_cap`, at bounded
  velocity, only via `agent_call`; everything else (revoke, complete, lift, canary) belongs
  to the task owner. Recovery sweeps can move funds **only to the recorded task owner**.
- **Out of scope / trusted:** the task owner's own key hygiene (a stolen task-owner key can
  recover that owner's wallets by design — that is ownership, not a vulnerability).
- **Assumption:** NEAR's account/key permission model behaves as documented (access-key
  method restriction, DeleteKey self-action rules). Live evidence supports these on Testnet.

## 6. Known limitations and residual risks

1. F-1 as above (CRITICAL, unmitigated in this RC).
2. No independent external audit; no formal verification; property-based fuzzing limited to
   the existing fuzz module.
3. Velocity breaker bounds *future* calls; within-cap call frequency is by design.
4. Agent-key allowance permits gas burn up to min(500 NEAR, wallet balance) — see F-A
   (previously documented as 0.5 NEAR).
5. Storage stake on wallets is sized for the embedded executor; wallet creation can fail on
   chain if funding is marginal (refund path exists and is tested; live incident documented
   and fixed by raising minimum funding).
6. Testnet evidence ≠ mainnet behavior guarantees (validator/economics differences).
7. **No production-readiness claim is made.** RC 5b4f600 is a hackathon release candidate.

## 7. References (proposed roadmap only)

- near-plugins (Upgradable + AccessControllable): https://github.com/Near-One/near-plugins —
  `near-plugins/src/upgradable.rs` (`up_stage_code`, `up_deploy_code`, staging-duration
  two-phase update), `near-plugins/src/access_control.rs` (role-gated up-* authorization)
- multisig controller: https://github.com/near/core-contracts/tree/master/multisig2 —
  `src/lib.rs` (`add_request`, `confirm`, FunctionCall-key membership, `SetNumConfirmations`)
- DAO-governance alternative (evaluated, not chosen):
  https://github.com/near-daos/sputnik-dao-contract — `sputnikdao2/src/proposals.rs`
  (`UpgradeSelf`/`UpgradeRemote`), `sputnikdao2/src/upgrade.rs` (2-week auto-update delay,
  migration rollback)
- Design report: `F1_MITIGATION_ARCHITECTURE.md` in this repository's release archive.

## 8. Responsible disclosure

Found a vulnerability? Please do NOT open a public issue. Contact the maintainers through
the hackathon project channel with reproduction details. We commit to acknowledging reports
within 48 hours, and to crediting reporters. Because this is a Testnet-stage project, no
bug bounty is offered; credit and public acknowledgment are.
