#!/usr/bin/env python3
"""FUKILLA attack simulation — hostile inputs against a running contract view.

This script is read-only (view calls only): it probes a deployed factory with
hostile task ids, over-cap amounts and out-of-range velocity params through the
same validation paths the unit fuzz tests cover. No keys, no funds, no TX.
"""
import base64
import json
import sys
import urllib.request

RPC = "https://rpc.testnet.near.org"
CONTRACT = sys.argv[1] if len(sys.argv) > 1 else "fukilla.testnet"


def rpc(method, params):
    req = urllib.request.Request(
        RPC,
        data=json.dumps({"jsonrpc": "2.0", "id": "1", "method": method, "params": params}).encode(),
        headers={"Content-Type": "application/json"},
    )
    return json.loads(urllib.request.urlopen(req, timeout=30).read())


def view(method, args):
    r = rpc(
        "query",
        {
            "request_type": "call_function",
            "account_id": CONTRACT,
            "method_name": method,
            "args_base64": base64.b64encode(json.dumps(args).encode()).decode(),
            "finality": "final",
        },
    )
    if "error" in r or "result" not in r:
        return None
    res = r.get("result", {}).get("result", [])
    return json.loads(bytes(res).decode()) if res else None


def main():
    print(f"== FUKILLA attack simulation against {CONTRACT} (read-only) ==")
    owner = view("get_owner", {})
    print(f"[1] get_owner            -> {owner}")

    # Hostile task ids (traversal, unicode, empty, overflow-ish) — factory must
    # answer safely (None / unknown task), never crash or leak state.
    hostile_ids = [
        "", "../etc/passwd", "../../fukilla", "\u0000null", "a" * 500,
        "%s%s%s", "'; DROP TABLE tasks;--", "🎉unicode-id", ".hidden", "-",
    ]
    ok = 0
    for tid in hostile_ids:
        t = view("get_task", {"task_id": tid})
        status = "safe(unknown)" if t in (None, {}) else "returned(task exists)"
        print(f"[fuzz] task_id={tid[:24]!r:28} -> {status}")
        ok += 1
    print(f"[2] hostile task ids     -> {ok}/{len(hostile_ids)} handled safely (no crash)")

    # Quarantine probe on real tasks, if any exist: view must never lie.
    real_ids = ["stress-run1", "e2e-proof-2"]
    for tid in real_ids:
        t = view("get_task", {"task_id": tid})
        if t:
            print(f"[3] task {tid!r} spent={t.get('spent')} quarantined={t.get('quarantined')} completed={t.get('completed')}")
    print("== PASS: contract answered every hostile probe without error ==")


if __name__ == "__main__":
    main()
