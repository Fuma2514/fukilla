#!/usr/bin/env node
// Generate an ed25519 keypair in NEAR format from a 32-byte seed.
// Usage: node gen-agent-key.js <seed-hex>  ->  prints {publicKey, secretKey}
const crypto = require('crypto');
const { ed25519 } = require('@noble/curves/ed25519');

function b58(buf) {
  const ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  let num = BigInt('0x' + buf.toString('hex'));
  let out = '';
  while (num > 0n) {
    out = ALPHABET[Number(num % 58n)] + out;
    num /= 58n;
  }
  for (const b of buf) {
    if (b === 0) out = '1' + out; else break;
  }
  return out;
}

const seedHex = process.argv[2];
const seed = Buffer.from(seedHex, 'hex');
const pub = ed25519.getPublicKey(seed);
console.log(JSON.stringify({
  publicKey: 'ed25519:' + b58(pub),
  secretKey: 'ed25519:' + b58(Buffer.concat([seed, pub])),
}));
