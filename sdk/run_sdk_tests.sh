#!/usr/bin/env bash
# Runs the SDK signer tests. The tests use a local no-network stub of
# near-api-js (sdk/test_stub/near-api-js) so they need no npm install.
set -euo pipefail
cd "$(dirname "$0")"
if [ ! -e node_modules/near-api-js ]; then
  mkdir -p node_modules
  ln -s ../test_stub/near-api-js node_modules/near-api-js
fi
node --experimental-strip-types --test sdk_owner_signer.test.ts "$@"
