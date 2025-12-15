# scripts

Helpers for the MCP agent shim and proof payloads.

- `host.py` — minimal MCP-like runner: hits the gateway, fetches & verifies receipt, saves it, builds public inputs (hash/policy/consent). Use with `python3 host.py --mode tool --amount 123 --anchor`.
- `TODO.md` — planned additions: Stwo prover hook, contract proof-hash anchor, MCP toolhost wiring.

For full public inputs with signature recovery (and proof via `receipt_sig` or an external prover), run the Rust CLI:
```bash
cd ../receipt_verifier
cargo run -- --receipt ../scripts/receipts/<id>.json --signature 0x... --gateway auto --policy_hash 0x... --consent_hash 0x... --out_public ../scripts/public.json --out_witness ../scripts/witness.json --out_proof ../scripts/proof.json --prove
```
- To use an external prover (e.g., LuminAIR), set `LUMINAIR_PROVER_CMD` to a command that reads stdin JSON `{public_inputs, witness}` and prints a Proof JSON. The CLI/toolhost will fall back to `receipt_sig` if the external prover is missing/fails.
- A ready CLI binary exists for external mode: `../receipt_verifier/target/debug/receipt_prover` (build with `cargo build`). Set `LUMINAIR_PROVER_CMD="../receipt_verifier/target/debug/receipt_prover"` to have `prove_receipt`/`auto_prove.py` call it.
