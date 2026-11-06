#!/usr/bin/env node
// Generate an ed25519 keypair in NEAR format from a 32-byte seed.
// Usage: node gen-agent-key.mjs <seed-hex>  ->  prints {publicKey, secretKey}
import { ed25519 } from '@noble/curves/ed25519.js';

import { b58 } from '/tmp/b58.mjs';

const seed = Buffer.from(process.argv[2], 'hex');
const pub = ed25519.getPublicKey(Uint8Array.from(seed));
console.log(JSON.stringify({
  publicKey: 'ed25519:' + b58(pub),
  secretKey: 'ed25519:' + b58(Buffer.concat([seed, pub])),
}));
