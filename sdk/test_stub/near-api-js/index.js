// Local test stub for near-api-js used by sdk unit tests (no network).
function KeyPair(ed25519) { this.ed25519 = ed25519; }
KeyPair.fromRandom = function () {
  const secret = "ed25519:" + Array.from({length: 88}, (_, i) =>
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"[(i * 7 + 13) % 63]).join("");
  const pk = "ed25519:stubpublickey";
  return {
    toString: () => secret,
    getPublicKey: () => ({ toString: () => pk }),
  };
};

async function connect(config) {
  return {
    __connectionConfig: config,
    // Record which account id the SDK asks to sign with, so tests can assert
    // that the signer resolves to ownerAccountId rather than the factory.
    async account(accountId) {
      return {
        accountId,
        async functionCall(args) {
          const last = connect.__lastCall;
          connect.__lastCall = { accountId, ...args };
          return { accountId, ...args };
        },
      };
    },
    connection: {
      provider: {
        async query(req) {
          const r = Buffer.from(JSON.stringify({ stub: true, req })).toString("base64");
          return { result: Array.from(Buffer.from(r)) };
        },
      },
    },
  };
}
connect.__lastCall = null;

module.exports = { KeyPair, connect };
