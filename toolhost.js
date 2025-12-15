#!/usr/bin/env node
// Minimal MCP toolhost for Claude Desktop.
// Tools:
// - payments.demo@1.0.0: proxy to gateway /mcp/tools/call
// - retrieval.demo@1.0.0: proxy to gateway /mcp/retrieval/query
// - verify_receipt: GET /verify/receipt/:id and /receipts/:id
// - anchor_receipt: POST /anchor/l2/:id then refetch receipt
// - prove_receipt: run cargo verifier to emit public/witness/proof (receipt_sig by default; external prover if configured)

import http from "http";
import { spawn } from "child_process";

const API = process.env.MCP_GATEWAY_API || "http://localhost:4005";
const CARGO_BIN = process.env.CARGO_BIN || "cargo";
const RECEIPT_DIR = process.env.RECEIPT_DIR || `${process.cwd()}/scripts/receipts`;
const PROVER_OUT_DIR = process.env.PROVER_OUT_DIR || `${process.cwd()}/scripts`;

function jsonResponse(res, code, obj) {
  res.writeHead(code, { "Content-Type": "application/json" });
  res.end(JSON.stringify(obj));
}

async function fetchJson(url, opts = {}) {
  const res = await fetch(url, { ...opts, headers: { "Content-Type": "application/json" } });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(`${res.status} ${JSON.stringify(data)}`);
  }
  return data;
}

async function proxyGateway({ tool, body }) {
  if (tool === "payments.demo@1.0.0") {
    return fetchJson(`${API}/mcp/tools/call`, {
      method: "POST",
      body: JSON.stringify({ tool_id: tool, auth_key: "demo", args: body.args || {} }),
    });
  }
  if (tool === "retrieval.demo@1.0.0") {
    return fetchJson(`${API}/mcp/retrieval/query`, {
      method: "POST",
      body: JSON.stringify({ query: body.query || "demo question", datasets: body.datasets || ["demo-ds-1"], auth_key: "demo" }),
    });
  }
  throw new Error("Unknown tool");
}

async function verifyReceipt(id) {
  const rcpt = await fetchJson(`${API}/receipts/${id}`);
  const verify = await fetchJson(`${API}/verify/receipt/${id}`);
  return { receipt: rcpt, verify };
}

async function anchorReceipt(id) {
  await fetchJson(`${API}/anchor/l2/${id}`, { method: "POST" });
  return verifyReceipt(id);
}

function runProver(receiptPath, signature, policyHash, consentHash, gateway = "auto") {
  return new Promise((resolve, reject) => {
    const args = [
      "run",
      "--",
      "--receipt",
      receiptPath,
      "--signature",
      signature,
      "--gateway",
      gateway,
      "--policy-hash",
      policyHash,
      "--consent-hash",
      consentHash,
      "--prove",
      "--out-public",
      `${PROVER_OUT_DIR}/public.json`,
      "--out-witness",
      `${PROVER_OUT_DIR}/witness.json`,
      "--out-proof",
      `${PROVER_OUT_DIR}/proof.json`,
    ];
    const proc = spawn(CARGO_BIN, args, { cwd: `${process.cwd()}/receipt_verifier` });
    let stdout = "";
    let stderr = "";
    proc.stdout.on("data", (d) => (stdout += d.toString()));
    proc.stderr.on("data", (d) => (stderr += d.toString()));
    proc.on("close", (code) => {
      if (code === 0) {
        try {
          const pub = JSON.parse(require("fs").readFileSync(`${PROVER_OUT_DIR}/public.json`, "utf8"));
          const wit = JSON.parse(require("fs").readFileSync(`${PROVER_OUT_DIR}/witness.json`, "utf8"));
          const proof = JSON.parse(require("fs").readFileSync(`${PROVER_OUT_DIR}/proof.json`, "utf8"));
          resolve({ pub, wit, proof, stdout });
        } catch (e) {
          reject(e);
        }
      } else {
        reject(new Error(`prover failed ${code}: ${stderr || stdout}`));
      }
    });
  });
}

const server = http.createServer(async (req, res) => {
  if (req.method !== "POST") return jsonResponse(res, 404, { error: "not found" });
  let body = "";
  req.on("data", (chunk) => (body += chunk));
  req.on("end", async () => {
    try {
      const { tool, args } = JSON.parse(body || "{}");
      if (!tool) throw new Error("missing tool");
      if (tool === "verify_receipt") {
        const out = await verifyReceipt(args.receipt_id);
        return jsonResponse(res, 200, out);
      }
      if (tool === "anchor_receipt") {
        const out = await anchorReceipt(args.receipt_id);
        return jsonResponse(res, 200, out);
      }
      if (tool === "prove_receipt") {
        const receiptPath = args.receipt_path;
        const sig = args.signature;
        const policyHash = args.policy_hash;
        const consentHash = args.consent_hash;
        const gateway = args.gateway || "auto";
        if (!receiptPath || !sig || !policyHash || !consentHash) {
          throw new Error("prove_receipt requires receipt_path, signature, policy_hash, consent_hash");
        }
        const proof = await runProver(receiptPath, sig, policyHash, consentHash, gateway);
        return jsonResponse(res, 200, proof);
      }
      // default: tool proxy
      const resp = await proxyGateway({ tool, body: args || {} });
      jsonResponse(res, 200, resp);
    } catch (e) {
      jsonResponse(res, 400, { error: e.message || String(e) });
    }
  });
});

const PORT = process.env.PORT || 4006;
server.listen(PORT, () => {
  console.log(`MCP toolhost listening on ${PORT}`);
  console.log(`Gateway: ${API}`);
});
