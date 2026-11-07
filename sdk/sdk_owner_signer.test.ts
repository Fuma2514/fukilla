/**
 * SDK test: proves the Fukilla SDK signer resolves to `ownerAccountId`
 * rather than the factory account — for BOTH create_wallet signing (GPT
 * audit finding A, fixed in e9642fb) and complete_task signing.
 *
 * Run: node --experimental-strip-types --test sdk/sdk_owner_signer.test.ts
 * (Node >= 22; near-api-js is satisfied by a local stub in node_modules so
 * no network or real chain access is needed.)
 */
import { test } from "node:test";
import assert from "node:assert/strict";
// @ts-ignore - local stub provided in node_modules
import { Fukilla } from "./index.ts";
import * as najs from "near-api-js";

const FACTORY = "fukilla.testnet";
const OWNER = "alice.owner.testnet";

function fakeConnection() {
  return najs.connect({ networkId: "testnet" });
}

test("createWallet SIGNS create_wallet with the configured ownerAccountId, not the factory (finding A)", async () => {
  const fuk = new Fukilla({ factory: FACTORY, ownerAccountId: OWNER, connection: await fakeConnection() });
  await fuk.createWallet({ fundingNEAR: 2, capNEAR: 0.1, ttlSec: 3600 });
  const last = (najs.connect as any).__lastCall;
  assert.ok(last, "a functionCall was issued");
  assert.equal(last.accountId, OWNER, "create_wallet signer must be ownerAccountId");
  assert.notEqual(last.accountId, FACTORY);
  assert.equal(last.methodName, "create_wallet");
  assert.equal(last.contractId, FACTORY);
});

test("createWallet reports ownerAccountId (configured), not the factory", async () => {
  const fuk = new Fukilla({ factory: FACTORY, ownerAccountId: OWNER, connection: await fakeConnection() });
  const task = await fuk.createWallet({ fundingNEAR: 2, capNEAR: 0.1, ttlSec: 3600 });
  assert.equal(task.ownerAccountId, OWNER);
  assert.notEqual(task.ownerAccountId, FACTORY);
});

test("createWallet falls back to factory as signer/owner when ownerAccountId not configured", async () => {
  const fuk = new Fukilla({ factory: FACTORY, connection: await fakeConnection() });
  const task = await fuk.createWallet({ fundingNEAR: 2, capNEAR: 0.1, ttlSec: 3600 });
  const last = (najs.connect as any).__lastCall;
  assert.equal(last.accountId, FACTORY);
  assert.equal(task.ownerAccountId, FACTORY);
});

test("completeTask signs with ownerAccountId — the recorded signer is the owner, not the factory", async () => {
  const conn = await fakeConnection();
  const fuk = new Fukilla({ factory: FACTORY, ownerAccountId: OWNER, connection: conn });
  await fuk.completeTask("task-abc", OWNER);
  const last = (najs.connect as any).__lastCall;
  assert.ok(last, "a functionCall was issued");
  assert.equal(last.accountId, OWNER, "signer account must be ownerAccountId");
  assert.notEqual(last.accountId, FACTORY);
  assert.equal(last.methodName, "complete_task");
  assert.equal(last.contractId, FACTORY);
});

test("owner round-trip: owner from createWallet handle is used to complete the task", async () => {
  const conn = await fakeConnection();
  const fuk = new Fukilla({ factory: FACTORY, ownerAccountId: OWNER, connection: conn });
  const task = await fuk.createWallet({ fundingNEAR: 2, capNEAR: 0.1, ttlSec: 3600 });
  await fuk.completeTask(task.taskId, task.ownerAccountId);
  const last = (najs.connect as any).__lastCall;
  assert.equal(last.accountId, task.ownerAccountId);
  assert.notEqual(last.accountId, FACTORY);
});
