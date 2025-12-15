# receipt_verifier

Rust CLI for receipt hashing, signature verification, and proof generation hooks (receipt_sig by default, external prover optional).

## What it does
- Canonicalize receipt JSON (sorted keys, no whitespace).
- Hash with SHA-256 (public `receipt_hash`).
- EIP-191 keccak + secp256k1 recover for the signature; can accept `--gateway auto` or a fixed address.
- Check policy/consent hashes.
- Emit public inputs + witness JSON for a proof circuit.
- Generate a proof:
  - Default: `receipt_sig` (packages the receipt signature).
  - External: set `LUMINAIR_PROVER_CMD` to call a prover that reads `{public_inputs, witness}` from stdin and returns a Proof JSON.

## Usage
```bash
cargo run -- \
  --receipt ../scripts/receipts/<id>.json \
  --signature 0x... \
  --gateway auto \
  --policy-hash 0x... \
  --consent-hash 0x... \
  --prove \
  --out-public ../scripts/public.json \
  --out-witness ../scripts/witness.json \
  --out-proof ../scripts/proof.json
```

## External prover (optional)
- Set `LUMINAIR_PROVER_CMD="your_prover_bin --arg1"`; it must read stdin JSON `{public_inputs, witness}` and write a Proof JSON to stdout. If unset or failing, the CLI/toolhost falls back to `receipt_sig`.
- Included: `receipt_prover` binary that does this I/O contract (currently wraps `receipt_sig`). Build with `cargo build` and set `LUMINAIR_PROVER_CMD="../receipt_verifier/target/debug/receipt_prover"`.

## Notes
- Signature covers the receipt excluding `receipt_sig` and `anchor` fields.
- Hashing: SHA-256 over canonical JSON; signature digest = keccak(canonical JSON) then EIP-191 keccak prefix.
- Replace `receipt_sig` with a real Stwo/LuminAIR prover when ready.
