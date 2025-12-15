# scripts

Helpers for the MCP agent shim and proof payloads.

- `host.py` — minimal MCP-like runner: hits the gateway, fetches & verifies receipt, saves it, builds public inputs (hash/policy/consent). Use with `python3 host.py --mode tool --amount 123 --anchor`.
- `TODO.md` — planned additions: Stwo prover hook, contract proof-hash anchor, MCP toolhost wiring.

For full public inputs with signature recovery (and proof via the `receipt_sig` hook), run the Rust CLI:
```bash
cd ../receipt_verifier
cargo run -- --receipt ../scripts/receipts/<id>.json --signature 0x... --gateway 0x... --policy_hash 0x... --consent_hash 0x... --out_public ../scripts/public.json --out_witness ../scripts/witness.json --out_proof ../scripts/proof.json --prove
```
