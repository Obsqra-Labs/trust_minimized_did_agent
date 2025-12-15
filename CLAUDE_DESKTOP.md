# Using this with Claude Desktop (MCP toolhost)

Tools exposed on port 4006 (node toolhost.js):
- `payments.demo@1.0.0` `{amount, description}` → gateway /mcp/tools/call
- `retrieval.demo@1.0.0` `{query, datasets}` → gateway /mcp/retrieval/query
- `verify_receipt` `{receipt_id}` → gateway verify + receipt
- `anchor_receipt` `{receipt_id}` → gateway /anchor/l2 then refetch
- `prove_receipt` `{receipt_path, signature, policy_hash, consent_hash, gateway? ("auto" ok)}` → runs the Rust verifier + proof hook (receipt_sig by default; external prover if `LUMINAIR_PROVER_CMD` set)

Setup:
1) Start toolhost (if not running): `node toolhost.js` (default port 4006).
2) In Claude Desktop, add an MCP server pointing to `http://localhost:4006`.
3) Call tools as above. The prove_receipt result includes public inputs, witness summary, and proof (receipt_sig or external).

Notes:
- Gateway base: `http://localhost:4005` (override via `MCP_GATEWAY_API`).
- Proofs are local-only; on-chain verification not required. Anchoring posts receipt hashes to Starknet via the gateway.
