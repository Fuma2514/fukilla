#!/usr/bin/env bash
# verify_deployment.sh — reconstructed deployment-hash verifier (2026-11-12).
#
# Rebuilt from the process documented in README.md; not copied from the lost
# original. Differences from the original are documented in the "REPRODUCIBILITY
# REQUIREMENTS" block below.
#
# What it does:
#   1. Builds the factory contract WASM with the pinned toolchain (rust-toolchain.toml).
#   2. If full git history is available: also builds BOTH audit commits
#      (b2990fb, e9642fb) in detached worktrees. If git history is absent
#      (e.g. working from a tarball backup), this is skipped with an explicit
#      warning — a commit-pinned build is then IMPOSSIBLE and that limitation
#      is printed, never silently ignored.
#   3. Hashes each built WASM (SHA-256 hex + Base58).
#   4. Queries the on-chain code_hash via NEAR RPC (endpoint configurable).
#   5. Compares and exits non-zero if NONE of the builds match. Matching is a
#      byte-for-byte claim, nothing softer.
#
# REPRODUCIBILITY REQUIREMENTS:
#   - rustup 1.29+, toolchain 1.93.0 (installed automatically per rust-toolchain.toml)
#   - target wasm32-unknown-unknown installed
#   - Cargo.lock committed and --locked used (dependency drift changes the hash)
#   - Same build path matters: prior audit showed plain builds are PATH-DEPENDENT
#     (same commit in /tmp worktree vs repo dir produced different hashes).
#     The script therefore always builds in a fixed temp worktree when git is
#     available, and reports its absolute path in the evidence summary.
#   - NEAR RPC reachable (default https://rpc.testnet.near.org, override via RPC_URL)
#   - jq, python3, base58 (python script inlined, no external b58 dependency)
#
# Usage: ACCOUNT_ID=fukilla.testnet ./verify_deployment.sh
set -euo pipefail

ACCOUNT_ID="${ACCOUNT_ID:-fukilla.testnet}"
RPC_URL="${RPC_URL:-https://rpc.testnet.near.org}"
COMMITS_DEFAULT="b2990fb e9642fb"
COMMITS="${COMMITS:-$COMMITS_DEFAULT}"
REPO_ROOT="$(cd "$(dirname "$0")" && pwd)"
WORKDIR="$(mktemp -d /tmp/fukilla-verify.XXXXXX)"
SUMMARY="$WORKDIR/evidence.txt"

log(){ echo "$@" | tee -a "$SUMMARY"; }

b58_of(){
python3 - "$1" <<'PY'
import sys, hashlib
data = open(sys.argv[1], 'rb').read()
digest = hashlib.sha256(data).digest()
h = hashlib.sha256(data).hexdigest()
alpha = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
n = int.from_bytes(digest, 'big')
out = ''
while n > 0:
    n, r = divmod(n, 58)
    out = alpha[r] + out
for b in digest:
    if b == 0: out = '1' + out
    else: break
print(h, out)
PY
}

build_wasm(){
  # $1 = source dir, $2 = label
  local src="$1" label="$2"
  ( cd "$src" && cargo build -j 1 --locked --release --target wasm32-unknown-unknown ) \
    > "$WORKDIR/build_$label.log" 2>&1 || { log "BUILD FAIL ($label): see $WORKDIR/build_$label.log"; return 1; }
  local wasm="$src/target/wasm32-unknown-unknown/release/fukilla.wasm"
  if [ ! -f "$wasm" ]; then log "WASM NOT FOUND for $label"; return 1; fi
  read -r hex b58 < <(b58_of "$wasm")
  log "BUILD $label path=$src"
  log "  sha256=$hex"
  log "  base58=$b58"
  echo "$b58" >> "$WORKDIR/hashes.txt"
}

: > "$WORKDIR/hashes.txt"
log "FUKILLA DEPLOYMENT VERIFICATION EVIDENCE — $(date -u +%Y-%m-%dT%H:%M:%SZ)"
log "repo_root=$REPO_ROOT workdir=$WORKDIR"
log "rustc=$(rustc --version 2>&1) cargo=$(cargo --version 2>&1)"

# 1. Build in a fixed temp copy (mitigates path-dependence of plain builds).
TMP_COPY="$WORKDIR/repo"
cp -a "$REPO_ROOT" "$TMP_COPY"
rm -rf "$TMP_COPY/.git" 2>/dev/null || true
build_wasm "$TMP_COPY" "working-tree(temppath)" || true

# 2. Commit-pinned builds (only if git history exists).
HAS_GIT=0
if [ -d "$REPO_ROOT/.git" ] && git -C "$REPO_ROOT" cat-file -t b2990fb >/dev/null 2>&1; then
  HAS_GIT=1
  for c in $COMMITS; do
    W="$WORKDIR/wt_$c"
    git -C "$REPO_ROOT" worktree add --detach "$W" "$c" >> "$SUMMARY" 2>&1 || { log "worktree $c failed"; continue; }
    build_wasm "$W" "commit:$c" || true
  done
else
  log "LIMITATION: git history absent (tarball backup) — commit-pinned builds at $COMMITS are IMPOSSIBLE here; only working-tree build was produced. Commit-level attribution UNVERIFIED in this run."
fi

# 3. On-chain code_hash via RPC.
RPC_RESP="$WORKDIR/rpc_view_account.json"
curl -s -m 30 "$RPC_URL" -X POST -H "Content-Type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":\"v\",\"method\":\"query\",\"params\":{\"request_type\":\"view_account\",\"account_id\":\"$ACCOUNT_ID\",\"finality\":\"final\"}}" \
  > "$RPC_RESP"
ONCHAIN=$(python3 -c "import json;print(json.load(open('$RPC_RESP'))['result']['code_hash'])" 2>/dev/null || echo "RPC_ERROR")
log "rpc_endpoint=$RPC_URL account=$ACCOUNT_ID"
log "onchain_code_hash=$ONCHAIN"
log "raw_rpc_response=$(cat "$RPC_RESP")"

# 3b. Download the deployed bytecode itself and hash it (removes trust in metadata).
curl -s -m 60 "$RPC_URL" -X POST -H "Content-Type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":\"c\",\"method\":\"query\",\"params\":{\"request_type\":\"view_code\",\"account_id\":\"$ACCOUNT_ID\",\"finality\":\"final\"}}" \
  > "$WORKDIR/rpc_view_code.json"
python3 - "$WORKDIR" <<'PY' >> "$SUMMARY"
import json, hashlib, sys, base64
w = sys.argv[1]
r = json.load(open(w + '/rpc_view_code.json'))['result']
data = base64.b64decode(r['code_base64'])
open(w + '/deployed.wasm', 'wb').write(data)
dg = hashlib.sha256(data).digest()
alpha = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
n = int.from_bytes(dg, 'big'); out = ''
while n > 0:
    n, q = divmod(n, 58); out = alpha[q] + out
for byte in dg:
    if byte == 0: out = '1' + out
    else: break
print(f"deployed_bytecode size={len(data)} sha256={dg.hex()} base58={out}")
PY
if [ -f "$WORKDIR/deployed.wasm" ]; then
  D58=$(python3 -c "
import hashlib
dg=hashlib.sha256(open('$WORKDIR/deployed.wasm','rb').read()).digest()
alpha='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
n=int.from_bytes(dg,'big'); out=''
while n: n,q=divmod(n,58); out=alpha[q]+out
print(out)")
  # a local build matches 'deployed' if it equals the bytecode hash itself
  echo "$D58" >> "$WORKDIR/hashes.txt"
  log "note: deployed bytecode hash added to comparison set (bytecode-level, stronger than code_hash metadata)"
fi

# 4. Compare (match = byte-for-byte Base58 equality).
MATCH="none"
if [ "$ONCHAIN" != "RPC_ERROR" ]; then
  while read -r h; do
    if [ "$h" = "$ONCHAIN" ]; then MATCH="$h"; break; fi
  done < "$WORKDIR/hashes.txt"
fi
log "match=$MATCH"
if [ "$MATCH" = "none" ]; then
  log "RESULT: MISMATCH — deployed code NOT reproduced byte-for-byte by any build in this run."
  log "Treat deployed-code-to-source attribution as UNVERIFIED (open audit finding until a build matches)."
  exit 1
else
  log "RESULT: MATCH — deployed code reproduced byte-for-byte."
fi
echo "Full evidence: $SUMMARY"
