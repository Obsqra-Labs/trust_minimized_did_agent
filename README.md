# Trust-Minimized DID Agent for MCP (Stwo/LuminAIR ready)

This repo is a standalone scaffold for a governed MCP agent that proves receipt integrity. It pairs a verified MCP gateway (proof-of-concept under development by Obsqra Labs) with a receipt-verification toolkit and a simple host/CLI.

It is intentionally generic: a DID-enabled agent with trust-minimized tool calls, signed/anchored receipts, and proof-ready payloads. Future hardening can add on-chain constraints or ZK guardrails for L1/L2; today the receipts are verified locally and can be anchored to L2.

## What this shows
- Agent (MCP host shim) calls governed tools via a verified MCP gateway (payments/retrieval).
- Gateway emits signed receipts; host verifies signature + policy/consent hashes.
- A Stwo-ready circuit spec to prove receipt validity (hash + signature + policy/consent equality) with public signals only.
- Optional on-chain anchor: Starknet Sepolia contract to bind receipt hashes; Voyager links.

## Repo layout
- `receipt_verifier/` — Rust crate scaffold for receipt hashing, signature check, and Stwo circuit integration.
- `scripts/` — helper scripts for the MCP host shim and payloads.

## Circuit spec (receipt verification)
**Public inputs**
- `receipt_hash` — sha256(canonical_receipt_json) (or keccak, pick one and stay consistent).
- `policy_hash`, `consent_hash` — from receipt.
- `gateway_address` — expected signer address (20 bytes, EVM style).
- `receipt_id` (optional) — hash of the id for provenance.
- `anchor_tx_hash` (optional) — L2 anchor binding.

**Witness**
- Canonical receipt JSON bytes (sorted keys, no whitespace).
- Receipt signature (r, s, v) as emitted by gateway (EIP-191 personal_sign over keccak(canonical)).
- Optional anchor block from receipt if you want to bind anchor_tx_hash.

**Constraints**
1) Hash: hash(canonical_json) == `receipt_hash` (use same hash as gateway; default sha256 here; switch to keccak if desired).
2) Signature: ECDSA verify over digest (keccak personal_sign of canonical bytes) → recover address → equals `gateway_address`.
3) Policy/consent equality: values inside receipt match public `policy_hash`/`consent_hash`.
4) Optional: `anchor_tx_hash` matches anchor section of receipt.
5) Optional: `receipt_id` hashed equals public `receipt_id` felt.

**Outputs / public signals**
- `receipt_hash`, `policy_hash`, `consent_hash`, `gateway_address`, `(anchor_tx_hash?)`, `(receipt_id_hash?)`.
- Proof object (Stwo style) for verifier (Rust/WASM or Giza-hosted).

## Fast path for a demo (local-only proofs)
1) Run the verified MCP gateway (POC) on :4005.
2) From `scripts/`, run:
   ```bash
   python3 auto_prove.py --mode tool --amount 123 --anchor --gateway auto
   ```
   - Calls governed tool, fetches receipt, optionally anchors to Starknet.
   - Runs Rust verifier: canonical hash + signature check + policy/consent check.
   - Emits public inputs (`public.json`), witness (`witness.json`), and proof (`proof.json`, currently the receipt signature hook).
3) Artifacts land in `scripts/receipts/`, `scripts/public.json`, `scripts/witness.json`, `scripts/proof.json`.
4) Proofs are local-only; on-chain proof verification is not required. You can still anchor the receipt hash to L2 for auditability.

## Components
- `scripts/auto_prove.py` — one-command E2E: gateway call → optional L2 anchor → verify → public/witness/proof (receipt_sig hook).
- `scripts/host.py` — minimal MCP-like runner (no prover) to call the gateway and save receipts/public inputs.
- `receipt_verifier/` — Rust CLI: canonicalize + hash + verify signature + emit public/witness, with a pluggable prover hook (currently `receipt_sig`).
- `toolhost.js` — Node MCP toolhost for Claude Desktop; exposes tools (`payments.demo@1.0.0`, `retrieval.demo@1.0.0`, `verify_receipt`, `anchor_receipt`, `prove_receipt`).

## Using the Node MCP toolhost (Claude Desktop)
1) Start it (already running on :4006; logs `/tmp/mcp_toolhost.log`). If needed:
   ```bash
   cd /opt/obsqura.fi/mcp_agent
   node toolhost.js
   ```
2) Add an MCP server in Claude Desktop pointing to `http://localhost:4006`.
3) Tools:
   - `payments.demo@1.0.0` `{amount, description}`
   - `retrieval.demo@1.0.0` `{query, datasets}`
   - `verify_receipt` `{receipt_id}`
   - `anchor_receipt` `{receipt_id}`
   - `prove_receipt` `{receipt_path, signature, policy_hash, consent_hash, gateway? ("auto" ok)}` → returns public/witness/proof (receipt_sig hook).

## Proof path (today)
- Hash: sha256 over canonical JSON (sorted keys, no whitespace).
- Signature: EIP-191 keccak + secp256k1 recovery; `--gateway auto` accepts recovered signer.
- Proof hook: `receipt_sig` (packs signature + public inputs). Swap it for a real Stwo/LuminAIR circuit when ready.
- On-chain: optional Starknet anchoring of receipt hash via gateway; no on-chain proof verification required.

## What this demonstrates
- DID-enabled, trust-minimized tool calls against a verified MCP gateway (POC).
- Receipts are signed, locally verified, and packaged for ZK-friendly public inputs.
- Optional L2 anchoring of receipt hashes for auditability; future path to on-chain constraints/proofs if desired.
- Agent integration: Claude Desktop can call the toolhost to run governed actions, anchor, and prepare proof artifacts without changing the gateway (see `CLAUDE_DESKTOP.md`).

## Starknet anchor (optional)
- Current contract: `0x072b5c3ff9d759f44350b689037a655842024ec1313db160feae16a3bc0df053` on Sepolia.
- After proof, you can reuse `/anchor/l2/:id` or extend the contract to store a proof hash.

## Notes
- Keep the hash function consistent across gateway, circuit, and signature digest. If gateway uses EIP-191 over keccak(canonical), mirror that; otherwise adopt sha256 everywhere.
- The crate below defaults to sha256 for the receipt hash and uses keccak(EIP-191) for signature recovery to match eth-account’s sign_message behavior.

## Quick host shim (no prover yet)
`scripts/host.py` is a minimal MCP-like runner:
- Calls your gateway tool/retrieval endpoints
- Fetches + verifies the receipt via `/verify/receipt/{id}`
- Saves the receipt to `scripts/receipts/<id>.json`
- Builds public inputs (hash/policy/consent/gateway) like the Rust CLI (Python version)
- Optional: anchors to L2 via `/anchor/l2/{id}`

Example:
```bash
cd scripts
python3 host.py --mode tool --amount 123 --description "demo payment" --anchor
# or
python3 host.py --mode retrieval --query "demo question" --datasets demo-ds-1
```
Outputs:
- Summary (receipt_id, status, anchor link if present)
- Receipt file path
- Public inputs JSON (for the Stwo circuit)
- If you need the proof stub + full public inputs, run the Rust CLI with `--prove --out_public ... --out_witness ... --out_proof ...` or simply call `scripts/auto_prove.py` (requires cargo + gateway running).
