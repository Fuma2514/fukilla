#!/usr/bin/env node
// Generate an ed25519 keypair in NEAR format from a 32-byte seed.
// Usage: node gen-agent-key.mjs <seed-hex>  ->  prints {publicKey, secretKey}
// Self-contained: base58 encoder inlined (no /tmp/b58.mjs dependency).
import { ed25519 } from '@noble/curves/ed25519.js';

const B58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

function b58(buf) {
  let num = BigInt('0x' + Buffer.from(buf).toString('hex'));
  let out = '';
  while (num > 0n) {
    out = B58_ALPHABET[Number(num % 58n)] + out;
    num /= 58n;
  }
  for (const b of buf) {
    if (b === 0) out = '1' + out; else break;
  }
  return out;
}

const seed = Buffer.from(process.argv[2], 'hex');
const pub = ed25519.getPublicKey(Uint8Array.from(seed));
console.log(JSON.stringify({
  publicKey: 'ed25519:' + b58(pub),
  secretKey: 'ed25519:' + b58(Buffer.concat([seed, pub])),
}));
