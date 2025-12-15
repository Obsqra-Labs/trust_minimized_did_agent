#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import sys
import urllib.request
from urllib.error import HTTPError
from typing import Any, Dict, Optional

API = os.environ.get("MCP_GATEWAY_API", "http://localhost:4005")


def req_json(method: str, path: str, body: Optional[Dict[str, Any]] = None) -> Any:
    url = f"{API}{path}"
    data = json.dumps(body).encode() if body is not None else None
    headers = {"Content-Type": "application/json"}
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req) as resp:  # nosec B310
            return json.loads(resp.read().decode())
    except HTTPError as e:
        try:
            detail = e.read().decode()
        except Exception:
            detail = str(e)
        raise RuntimeError(f"{method} {path} failed: {e.code} {detail}")


def canonical_json(val: Any) -> Any:
    if isinstance(val, dict):
        return {k: canonical_json(val[k]) for k in sorted(val.keys())}
    if isinstance(val, list):
        return [canonical_json(v) for v in val]
    return val


def sha256_hex(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def eip191_keccak(data: bytes) -> bytes:
    prefix = f"\u0019Ethereum Signed Message:\n{len(data)}".encode()
    import sha3  # pysha3

    k = sha3.keccak_256()
    k.update(prefix)
    k.update(data)
    return k.digest()


def build_public_inputs(receipt: Dict[str, Any]) -> Dict[str, Any]:
    canon = canonical_json(receipt)
    canon_str = json.dumps(canon, separators=(",", ":"))
    rcpt_hash = sha256_hex(canon_str)
    return {
        "receipt_hash": f"0x{rcpt_hash}",
        "policy_hash": receipt.get("policy_hash"),
        "consent_hash": receipt.get("consent_snapshot_hash"),
        "gateway_address": None,  # not derivable without sig recovery in Python stdlib
        "note": "Run receipt_verifier (Rust) for sig recovery and full public inputs",
    }


def save_receipt(receipt: Dict[str, Any]) -> str:
    os.makedirs("receipts", exist_ok=True)
    rid = receipt.get("receipt_id", "receipt")
    path = os.path.join("receipts", f"{rid}.json")
    with open(path, "w") as f:
        json.dump(receipt, f, indent=2)
    return path


def main():
    parser = argparse.ArgumentParser(description="Minimal MCP host shim → gateway → receipt")
    parser.add_argument("--mode", choices=["tool", "retrieval"], required=True)
    parser.add_argument("--amount", type=int, default=100, help="amount for tool mode")
    parser.add_argument("--description", default="demo payment", help="description for tool mode")
    parser.add_argument("--query", default="demo question", help="query for retrieval")
    parser.add_argument("--datasets", default="demo-ds-1", help="comma-separated datasets")
    parser.add_argument("--anchor", action="store_true", help="anchor to L2 after receipt")
    args = parser.parse_args()

    if args.mode == "tool":
        body = {
            "tool_id": "payments.demo@1.0.0",
            "args": {"amount": args.amount, "description": args.description},
            "auth_key": "demo",
        }
        resp = req_json("POST", "/mcp/tools/call", body)
    else:
        ds = [d.strip() for d in args.datasets.split(",") if d.strip()]
        body = {"query": args.query, "datasets": ds, "auth_key": "demo"}
        resp = req_json("POST", "/mcp/retrieval/query", body)

    receipt_id = resp.get("receipt_id")
    if not receipt_id:
        print("No receipt_id returned", file=sys.stderr)
        sys.exit(1)

    receipt = req_json("GET", f"/receipts/{receipt_id}")
    verify = req_json("GET", f"/verify/receipt/{receipt_id}")

    if args.anchor:
        try:
            _ = req_json("POST", f"/anchor/l2/{receipt_id}")
            receipt = req_json("GET", f"/receipts/{receipt_id}")
        except Exception as e:
            print(f"Anchor failed: {e}", file=sys.stderr)

    path = save_receipt(receipt)
    pub_inputs = build_public_inputs(receipt)

    print("=== Gateway call complete ===")
    print(f"receipt_id: {receipt_id}")
    print(f"verify.ok: {verify.get('ok')}, sig_ok: {verify.get('sig_ok')}, snapshot_ok: {verify.get('snapshot_ok')}")
    anchor = receipt.get("anchor")
    if anchor:
        l2 = anchor.get("l2_tx", {})
        print(f"anchor: {anchor.get('anchor_id')} tx: {l2.get('tx_hash')}")
    print(f"receipt saved: {path}")
    print("public inputs (hash/policy/consent):")
    print(json.dumps(pub_inputs, indent=2))
    print("Note: run the Rust receipt_verifier CLI for signature recovery + full public inputs/proof")


if __name__ == "__main__":
    main()
