#!/usr/bin/env python3
"""
End-to-end helper:
- Calls the gateway (tool or retrieval) to get a receipt.
- Optionally anchors to L2.
- Runs the Rust receipt_verifier CLI to verify + emit public inputs/witness + proof (currently uses the receipt_sig hook).

Requires:
- Gateway running on http://localhost:4005 (override with MCP_GATEWAY_API env).
- receipt_verifier Rust crate (cargo) available.
- A gateway signer address (use --gateway or env GATEWAY_ADDR).
"""
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from dotenv import load_dotenv

# Reuse helper funcs from host.py
sys.path.append(str(Path(__file__).parent))
import host  # type: ignore

# Load env for optional external prover (LUMINAIR_PROVER_CMD)
load_dotenv()
LUMI_CMD = os.environ.get("LUMINAIR_PROVER_CMD")


def run_gateway_call(mode: str, amount: int, description: str, query: str, datasets, anchor: bool):
    if mode == "tool":
        body = {
            "tool_id": "payments.demo@1.0.0",
            "args": {"amount": amount, "description": description},
            "auth_key": "demo",
        }
        resp = host.req_json("POST", "/mcp/tools/call", body)
    else:
        ds = [d.strip() for d in datasets.split(",") if d.strip()]
        body = {"query": query, "datasets": ds, "auth_key": "demo"}
        resp = host.req_json("POST", "/mcp/retrieval/query", body)
    receipt_id = resp.get("receipt_id")
    if not receipt_id:
        raise RuntimeError("No receipt_id returned")
    receipt = host.req_json("GET", f"/receipts/{receipt_id}")
    verify = host.req_json("GET", f"/verify/receipt/{receipt_id}")
    if anchor:
        try:
            _ = host.req_json("POST", f"/anchor/l2/{receipt_id}")
            receipt = host.req_json("GET", f"/receipts/{receipt_id}")
        except Exception as e:
            print(f"Anchor failed: {e}", file=sys.stderr)
    path = host.save_receipt(receipt)
    return receipt_id, receipt, verify, path


def run_cargo(receipt_path: Path, signature: str, gateway: str, policy_hash: str, consent_hash: str):
    base = Path(__file__).resolve().parent
    crate_dir = base.parent / "receipt_verifier"
    out_public = base / "public.json"
    out_witness = base / "witness.json"
    out_proof = base / "proof.json"
    cmd = [
        "cargo",
        "run",
        "--",
        "--receipt",
        str(receipt_path),
        "--signature",
        signature,
        "--gateway",
        gateway,
        "--policy-hash",
        policy_hash,
        "--consent-hash",
        consent_hash,
        "--prove",
        "--out-public",
        str(out_public),
        "--out-witness",
        str(out_witness),
        "--out-proof",
        str(out_proof),
    ]
    if not LUMI_CMD:
        cmd.append("--stub")
    print("Running verifier CLI:\n", " ".join(cmd))
    subprocess.run(cmd, check=True, cwd=crate_dir)
    return out_public, out_witness, out_proof


def main():
    parser = argparse.ArgumentParser(description="Call gateway and run verifier CLI automatically")
    parser.add_argument("--mode", choices=["tool", "retrieval"], required=True)
    parser.add_argument("--amount", type=int, default=100)
    parser.add_argument("--description", default="demo payment")
    parser.add_argument("--query", default="demo question")
    parser.add_argument("--datasets", default="demo-ds-1")
    parser.add_argument("--anchor", action="store_true")
    parser.add_argument(
        "--gateway",
        default=os.environ.get("GATEWAY_ADDR"),
        help="gateway signer address (0x...); required",
    )
    args = parser.parse_args()

    if not args.gateway:
        print("Missing gateway address (set --gateway or GATEWAY_ADDR)", file=sys.stderr)
        sys.exit(1)

    receipt_id, receipt, verify, path = run_gateway_call(
        args.mode, args.amount, args.description, args.query, args.datasets, args.anchor
    )
    sig = receipt.get("receipt_sig")
    policy_hash = receipt.get("policy_hash")
    consent_hash = receipt.get("consent_snapshot_hash")
    if not sig or not policy_hash or not consent_hash:
        print("Missing sig/policy/consent in receipt", file=sys.stderr)
        sys.exit(1)

    print(f"Receipt: {receipt_id}")
    print(f"verify.ok={verify.get('ok')} sig_ok={verify.get('sig_ok')} snapshot_ok={verify.get('snapshot_ok')}")
    anchor = receipt.get("anchor")
    if anchor:
        l2 = anchor.get("l2_tx", {})
        print(f"anchor: {anchor.get('anchor_id')} tx: {l2.get('tx_hash')}")

    out_public, out_witness, out_proof = run_cargo(Path(path).resolve(), sig, args.gateway, policy_hash, consent_hash)
    print("=== Done ===")
    print(f"receipt saved: {path}")
    print(f"public inputs: {out_public}")
    print(f"witness: {out_witness}")
    print(f"proof: {out_proof}")


if __name__ == "__main__":
    main()
