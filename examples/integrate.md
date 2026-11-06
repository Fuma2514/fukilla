# Integrating FUKILLA in 3 Steps

Already building an AI agent on NEAR? Add sandboxed
payments in under 10 minutes.

## Step 1: Deploy the factory

```bash
cargo near build non-reproducible-wasm
near deploy fukilla.testnet \
  target/wasm32-unknown-unknown/release/fukilla.wasm
```

## Step 2: Create a task wallet for your agent

```bash
near contract call-function as-transaction fukilla.testnet create_wallet \
  json-args '{
    "task_id": "my-agent-task-001",
    "agent_key": "<YOUR_AGENT_PUBLIC_KEY>",
    "ttl_sec": 3600,
    "spend_cap": "1000000000000000000000000",
    "max_calls_per_window": 10,
    "velocity_window_sec": 60
  }' \
  prepaid-gas "500 Tgas" \
  attached-deposit "2 NEAR" \
  sign-as you.testnet network-config testnet sign-with-legacy-keychain send
```

## Step 3: Give your agent the function-call key

Your agent now has a key that:
- Can only call `agent_call` on the factory
- Cannot transfer NEAR directly
- Cannot exceed spend_cap
- Self-quarantines on suspicious behavior

That's it. No backend. No oracle. No server to maintain.

## What your agent calls

```python
near.call("fukilla.testnet", "agent_call", {
    "task_id": "my-agent-task-001",
    "receiver_id": "vendor.near",
    "amount": "100000000000000000000000",
    "memo": "API payment"
})
```
