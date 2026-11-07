#!/usr/bin/env bash
# FUKILLA — external auditor deployment verification
#
# Builds the factory contract with the pinned toolchain (rust-toolchain.toml:
# 1.93.0 + wasm32-unknown-unknown, -j 1) at BOTH audit commits —
#   b2990fb  (content deployed on-chain per tx GEwkF97Q1gdu6jHRu7hZCNpAsXhqAqunwZD69YtSbs2w)
#   e9642fb  (HEAD: finding A SDK fix; factory contract identical to b2990fb)
# computes each WASM's SHA-256 (hex + Base58), queries the on-chain code hash
# via RPC, and reports which commit (if any) matches. Exits non-zero if NEITHER
# build matches the on-chain code hash.
#
# Usage: ./verify_deployment.sh
#   ACCOUNT_ID env var overrides the default (fukilla.testnet, identified from
#   README.md + deploy tx GEwkF97Q1gdu6jHRu7hZCNpAsXhqAqunwZD69YtSbs2w).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$REPO_ROOT"

ACCOUNT_ID="${ACCOUNT_ID:-fukilla.testnet}"
RPC_URL="${RPC_URL:-https://rpc.testnet.near.org}"
COMMITS="${VERIFY_COMMITS:-b2990fb e9642fb}"
BUILD_CMD="cargo build -j 1 --locked --release --target wasm32-unknown-unknown"

command -v cargo >/dev/null 2>&1 || export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found"; exit 2; }
command -v git >/dev/null 2>&1 || { echo "ERROR: git not found"; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "ERROR: python3 not found"; exit 2; }

base58_hex() {
python3 - "$1" <<'PY'
import sys
alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
num = int(sys.argv[1], 16)
out = ""
while num > 0:
    num, r = divmod(num, 58)
    out = alphabet[r] + out
for b in bytes.fromhex(sys.argv[1]):
    if b == 0:
        out = "1" + out
    else:
        break
print(out or "1")
PY
}

echo "=== On-chain code hash ==="
RAW_RPC="$(curl -s "$RPC_URL" -H 'Content-Type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":\"fukilla-verify\",\"method\":\"query\",\"params\":{\"request_type\":\"view_account\",\"account_id\":\"$ACCOUNT_ID\",\"finality\":\"final\"}}")"
echo "Raw RPC response: $RAW_RPC"
ONCHAIN_B58="$(printf '%s' "$RAW_RPC" | python3 -c "import json,sys; print(json.load(sys.stdin)['result']['code_hash'])")"
echo "On-chain code hash: $ONCHAIN_B58   (account: $ACCOUNT_ID, endpoint: $RPC_URL)"
echo

MATCHING_COMMIT=""
ANY_MATCH=0
for C in $COMMITS; do
  SHA_FULL="$(git rev-parse "$C")"
  WORKTREE="$(mktemp -d /tmp/fukilla-verify.XXXXXX)"
  echo "=== Building commit $C ($SHA_FULL) ==="
  git worktree add --detach "$WORKTREE" "$SHA_FULL" >/dev/null
  trap 'git worktree remove --force "$WORKTREE" 2>/dev/null || true' EXIT
  WPATH="$WORKTREE/target/wasm32-unknown-unknown/release/fukilla.wasm"
  ( cd "$WORKTREE" && $BUILD_CMD >/dev/null )
  test -f "$WPATH" || { echo "ERROR: $WASM_PATH not produced for $C"; exit 2; }
  HEX="$(sha256sum "$WPATH" | awk '{print $1}')"
  B58="$(base58_hex "$HEX")"
  if [ "$B58" = "$ONCHAIN_B58" ]; then
    VERDICT="MATCH — on-chain code hash matches commit $C"
    MATCHING_COMMIT="$C"
    ANY_MATCH=1
  else
    VERDICT="MISMATCH — local $B58 != on-chain $ONCHAIN_B58"
  fi
  echo "WASM SHA-256 (hex):    $HEX"
  echo "WASM SHA-256 (Base58): $B58"
  echo "Verdict: $VERDICT"
  git worktree remove --force "$WORKTREE"
  trap - EXIT
  echo
done

echo "=== Summary ==="
echo "repo                  : $(git remote get-url origin 2>/dev/null || echo 'local')"
echo "HEAD                  : $(git rev-parse HEAD) ($(git log -1 --format=%s))"
echo "source commit(s)      : $COMMITS"
echo "matching commit       : ${MATCHING_COMMIT:-NONE — on-chain code hash matches neither build (AUDIT FINDING: report both hashes explicitly)}"
echo "rustc version         : $(rustc --version)"
echo "cargo version         : $(cargo --version)"
CARGO_NEAR_V="$(cargo near --version 2>/dev/null || true)"
if [ -n "$CARGO_NEAR_V" ]; then echo "cargo-near version    : $CARGO_NEAR_V"; else echo "cargo-near version    : not used (built with plain pinned cargo)"; fi
echo "build command         : $BUILD_CMD (per commit, in a detached git worktree)"
echo "rpc endpoint          : $RPC_URL"
echo "rpc raw response      : $RAW_RPC"
echo "on-chain code hash    : $ONCHAIN_B58"
echo "verification result   : $([ $ANY_MATCH -eq 1 ] && echo MATCH || echo MISMATCH)"

[ $ANY_MATCH -eq 1 ] || exit 1
exit 0
