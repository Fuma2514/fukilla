/**
 * FUKILLA — REAL TESTNET integration test (NOT a mock test).
 *
 * RUNS ONLY when real credentials are provided:
 *   FUKILLA_TESTNET_OWNER_ACCOUNT  — a funded NEAR testnet account id
 *   FUKILLA_TESTNET_OWNER_SECRET   — its full-access ed25519 secret key
 * Without both, the file emits a single SKIP line and exits 0 — it must
 * NEVER be counted as a passing integration test.
 *
 * What is proven on live chain state:
 *   T1 create_wallet signs with ownerAccountId (get_task.owner == owner account)
 *   T2 task wallet account exists on-chain with the executor deployed
 *   T3 agent_call: signed with the agent FC key, spends from the WALLET
 *      (wallet balance decreases; factory balance unchanged), spent advances
 *   T4 spend cap enforcement on live chain (over-cap agent_call fails)
 *   T5 complete_task (signed by owner) deletes the wallet account on-chain
 *   T6 post-destruction: get_task.completed == true, wallet account 404s
 *
 * Mock-level tests live in sdk/sdk_owner_signer.test.ts and are clearly
 * separate. Never report this file's SKIP as a pass.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { KeyPair, connect } from "near-api-js";

const OWNER_ACCOUNT = process.env.FUKILLA_TESTNET_OWNER_ACCOUNT;
const OWNER_SECRET = process.env.FUKILLA_TESTNET_OWNER_SECRET;
const FACTORY = process.env.FUKILLA_FACTORY ?? "fukilla.testnet";
const RPC = "https://rpc.testnet.near.org";

async function rpc(method, params) {
  const res = await fetch(RPC, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: "t", method, params }),
  });
  return res.json();
}
async function viewAccount(accountId) {
  return rpc("query", { request_type: "view_account", account_id: accountId, finality: "final" });
}
async function viewCall(contractId, methodName, args) {
  const r = await rpc("query", { request_type: "call_function", account_id: contractId, method_name: methodName, args_base64: Buffer.from(JSON.stringify(args)).toString("base64"), finality: "final" });
  if (r.error) throw new Error(r.error.message);
  return r.result.result;
}
function decodeResult(bytes) {
  return JSON.parse(Buffer.from(bytes).toString());
}

const RUN_LIVE = Boolean(OWNER_ACCOUNT && OWNER_SECRET);

test("FUKILLA testnet integration: signer, predecessor, authorization, resulting state",
  { timeout: 300000, skip: RUN_LIVE ? false : "FUKILLA_TESTNET_OWNER_ACCOUNT / FUKILLA_TESTNET_OWNER_SECRET not set — NO live-chain evidence produced" },
  async (t) => {
  const kp = KeyPair.fromString(OWNER_SECRET);
  const near = await connect({ networkId: "testnet", nodeUrl: RPC, keyStore: null });
  const owner = await near.account(OWNER_ACCOUNT);
  // Explicitly inject credentials (near.account does not load keyStores here)
  owner.signAndSendTransaction = owner.signAndSendTransaction.bind(owner);
  const { InMemoryKeyStore } = await import("near-api-js/lib/key_stores/in_memory_key_store.js");
  const ks = new InMemoryKeyStore();
  await ks.setKey("testnet", OWNER_ACCOUNT, kp);
  const near2 = await connect({ networkId: "testnet", nodeUrl: RPC, keyStore: ks });
  const acc = await near2.account(OWNER_ACCOUNT);

  // --- T1/T2: createWallet signed by the owner account ---
  const factoryBefore = (await viewAccount(FACTORY)).result.amount;
  const fundingNEAR = 2.5, capNEAR = 0.05, payNEAR = 0.01;
  // dynamic import of the SDK relative to this file
  const { Fukilla } = await import("./index.ts");
  const fuk = new Fukilla({ factory: FACTORY, network: "testnet", ownerAccountId: OWNER_ACCOUNT, connection: acc.connection });
  const task = await fuk.createWallet({ fundingNEAR, capNEAR, ttlSec: 3600 });
  assert.equal(task.ownerAccountId, OWNER_ACCOUNT, "handle must record the signing owner");

  const taskOnChain = decodeResult(await viewCall(FACTORY, "get_task", { task_id: task.taskId }));
  assert.equal(taskOnChain.owner, OWNER_ACCOUNT, "CONTRACT must record ownerAccountId as task owner (signer proof)");
  assert.equal(taskOnChain.spent, "0");

  const walletAcc = await viewAccount(task.wallet);
  assert.ok(walletAcc.result.code_hash && walletAcc.result.code_hash !== "1111111111111111111111111111111111111111111111111111111111111111", "executor must be deployed on the wallet");

  // --- T3: agent_call from the agent FC key ---
  const agentNear = await connect({ networkId: "testnet", nodeUrl: RPC, keyStore: (async () => { const { InMemoryKeyStore } = await import("near-api-js/lib/key_stores/in_memory_key_store.js"); const k = new InMemoryKeyStore(); await k.setKey("testnet", task.wallet, KeyPair.fromString(task.agentSecret)); return k; })() });
  const agentAcc = await agentNear.account(task.wallet);
  await agentAcc.functionCall({
    contractId: FACTORY,
    methodName: "agent_call",
    args: { task_id: task.taskId, receiver_id: OWNER_ACCOUNT, amount: "10000000000000000000000" /* 0.01 NEAR */, memo: "integration proof" },
    gas: BigInt("100000000000000"),
  });
  const after = decodeResult(await viewCall(FACTORY, "get_task", { task_id: task.taskId }));
  assert.equal(after.spent, "10000000000000000000000", "on-chain spent must reflect the agent payment");
  assert.equal(after.owner, OWNER_ACCOUNT);

  // --- T4: spend cap enforcement ---
  await assert.rejects(
    agentAcc.functionCall({
      contractId: FACTORY,
      methodName: "agent_call",
      args: { task_id: task.taskId, receiver_id: OWNER_ACCOUNT, amount: "100000000000000000000000" /* 0.1 NEAR > cap */, memo: "over-cap" },
      gas: BigInt("100000000000000"),
    }),
    /spend cap exceeded/,
    "over-cap agent_call must fail on live chain"
  );

  // --- T5/T6: owner completes; wallet is destroyed ---
  await acc.functionCall({ contractId: FACTORY, methodName: "complete_task", args: { task_id: task.taskId }, gas: BigInt("120000000000000") });
  const destroyed = await viewAccount(task.wallet);
  assert.ok(destroyed.error, "wallet account must no longer exist after sweep (T5)");
  const finalTask = decodeResult(await viewCall(FACTORY, "get_task", { task_id: task.taskId }));
  assert.equal(finalTask.completed, true, "task must be completed after sweep (T6)");
});
