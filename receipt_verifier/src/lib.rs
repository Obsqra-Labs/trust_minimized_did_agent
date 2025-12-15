use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tiny_keccak::{Hasher, Keccak};
use k256::ecdsa::{Signature as KSig, RecoveryId, VerifyingKey};
use hex::FromHex;
use thiserror::Error;
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::io::Write;

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("invalid hex: {0}")] Hex(String),
    #[error("serde error: {0}")] Serde(String),
    #[error("signature error: {0}")] Sig(String),
    #[error("address mismatch")] AddressMismatch,
    #[error("policy/consent mismatch")] PolicyConsentMismatch,
    #[error("gateway parse error: {0}")] GatewayParse(String),
    #[error("prover error: {0}")] Prover(String),
}

#[derive(Debug, Deserialize, Clone)]
pub struct Receipt {
    #[serde(flatten)]
    pub rest: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicInputs {
    pub receipt_hash: String,
    pub policy_hash: String,
    pub consent_hash: String,
    pub gateway_address: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Witness {
    pub canonical_receipt: String,
    pub signature_hex: String,
    pub receipt_id: Option<String>,
    pub anchor_tx_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    pub proof_id: String,
    pub proof: String,
    pub public_inputs: PublicInputs,
    pub witness_summary: WitnessSummary,
    pub prover: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessSummary {
    pub receipt_id: Option<String>,
    pub anchor_tx_hash: Option<String>,
    pub canonical_len: usize,
}

pub fn normalize_hex_even(s: &str) -> String {
    let clean = s.trim_start_matches("0x");
    if clean.len() % 2 == 0 {
        clean.to_string()
    } else {
        format!("0{}", clean)
    }
}

/// Canonicalize JSON by sorting keys recursively.
pub fn canonical_json(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::Object(map) => {
            let mut sorted = BTreeMap::new();
            for (k, v) in map.iter() {
                sorted.insert(k.clone(), canonical_json(v));
            }
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonical_json).collect())
        }
        _ => val.clone(),
    }
}

/// Compute sha256 over canonical JSON string (no whitespace, sorted keys).
pub fn receipt_hash_sha256(val: &serde_json::Value) -> String {
    let canon = canonical_json(val);
    let s = serde_json::to_string(&canon).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Ethereum personal_sign hash (EIP-191) over bytes.
pub fn personal_hash_keccak(bytes: &[u8]) -> [u8; 32] {
    let prefix = format!("\u{19}Ethereum Signed Message:\n{}", bytes.len());
    let mut k = Keccak::v256();
    let mut out = [0u8; 32];
    k.update(prefix.as_bytes());
    k.update(bytes);
    k.finalize(&mut out);
    out
}

/// Keccak-256 over arbitrary bytes.
pub fn keccak256(bytes: &[u8]) -> [u8; 32] {
    let mut k = Keccak::v256();
    let mut out = [0u8; 32];
    k.update(bytes);
    k.finalize(&mut out);
    out
}

/// Recover address from a 65-byte signature (r,s,v) over given hash.
pub fn recover_address(sig_hex: &str, msg_hash: [u8; 32]) -> Result<[u8; 20], VerifyError> {
    let sig_bytes = Vec::from_hex(normalize_hex_even(sig_hex))
        .map_err(|e| VerifyError::Hex(e.to_string()))?;
    if sig_bytes.len() != 65 {
        return Err(VerifyError::Sig("expected 65-byte signature".into()));
    }
    let v = sig_bytes[64];
    let rec_byte = match v {
        27 | 28 => v - 27,
        _ => v % 4,
    };
    let rec_id = RecoveryId::from_byte(rec_byte).ok_or_else(|| VerifyError::Sig("bad recovery id".into()))?;
    let rsig = KSig::from_slice(&sig_bytes[..64])
        .map_err(|e| VerifyError::Sig(e.to_string()))?
        ;
    let vk = VerifyingKey::recover_from_prehash(&msg_hash, &rsig, rec_id)
        .map_err(|e| VerifyError::Sig(e.to_string()))?;
    let pubkey_bytes = vk.to_encoded_point(false);
    let mut k = Keccak::v256();
    let mut out = [0u8; 32];
    k.update(&pubkey_bytes.as_bytes()[1..]);
    k.finalize(&mut out);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&out[12..]);
    Ok(addr)
}

/// Verify the receipt signature and policy/consent hashes against expected values.
pub fn verify_receipt(
    receipt_val: &serde_json::Value,
    receipt_sig_hex: &str,
    expected_gateway: Option<[u8; 20]>,
    expected_policy_hash: &str,
    expected_consent_hash: &str,
) -> Result<(String, [u8; 20]), VerifyError> {
    // Strip fields not covered by the signature (receipt_sig, anchor).
    let mut base = receipt_val.clone();
    if let Some(obj) = base.as_object_mut() {
        obj.remove("receipt_sig");
        obj.remove("anchor");
    }
    let canon = canonical_json(&base);
    let canon_str = serde_json::to_string(&canon).map_err(|e| VerifyError::Serde(e.to_string()))?;
    // hash for public signal
    let rcpt_hash = receipt_hash_sha256(&canon);
    // personal_sign digest for signature recovery: keccak(canonical_json) then EIP-191 keccak
    let digest_bytes = keccak256(canon_str.as_bytes());
    let digest = personal_hash_keccak(&digest_bytes);
    let addr = recover_address(receipt_sig_hex, digest)?;
    if let Some(exp) = expected_gateway {
        if addr != exp {
            return Err(VerifyError::AddressMismatch);
        }
    }
    // Check policy/consent fields inside receipt if present
    let policy_ok = receipt_val.get("policy_hash").and_then(|v| v.as_str()) == Some(expected_policy_hash);
    let consent_ok = receipt_val.get("consent_snapshot_hash").and_then(|v| v.as_str()) == Some(expected_consent_hash);
    if !(policy_ok && consent_ok) {
        return Err(VerifyError::PolicyConsentMismatch);
    }
    Ok((rcpt_hash, addr))
}

/// Compute public inputs and witness for downstream Stwo circuit.
pub fn build_public_and_witness(
    receipt_val: &serde_json::Value,
    signature_hex: &str,
    gateway_hex: Option<&str>,
) -> Result<(PublicInputs, Witness), VerifyError> {
    let expected_gateway = if let Some(gw) = gateway_hex {
        let mut gateway_bytes = [0u8; 20];
        let clean = normalize_hex_even(gw);
        let gb = Vec::from_hex(&clean).map_err(|e| VerifyError::GatewayParse(e.to_string()))?;
        let slice: &[u8] = if gb.len() == 32 { &gb[12..] } else { &gb };
        if slice.len() != 20 {
            return Err(VerifyError::GatewayParse(format!("gateway must be 20 bytes (or 32 felt), got {}", gb.len())));
        }
        gateway_bytes.copy_from_slice(slice);
        Some(gateway_bytes)
    } else {
        None
    };
    let (rcpt_hash, addr) = verify_receipt(
        receipt_val,
        signature_hex,
        expected_gateway,
        receipt_val.get("policy_hash").and_then(|v| v.as_str()).unwrap_or_default(),
        receipt_val.get("consent_snapshot_hash").and_then(|v| v.as_str()).unwrap_or_default(),
    )?;
    let canon = canonical_json(receipt_val);
    let canon_str = serde_json::to_string(&canon).map_err(|e| VerifyError::Serde(e.to_string()))?;
    let pub_inputs = PublicInputs {
        receipt_hash: format!("0x{}", rcpt_hash),
        policy_hash: receipt_val
            .get("policy_hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        consent_hash: receipt_val
            .get("consent_snapshot_hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        gateway_address: format!("0x{}", hex::encode(addr)),
        note: Some("Use these as public signals; feed canonical_receipt + sig as witness".into()),
    };
    let witness = Witness {
        canonical_receipt: canon_str,
        signature_hex: signature_hex.to_string(),
        receipt_id: receipt_val.get("receipt_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        anchor_tx_hash: receipt_val
            .get("anchor")
            .and_then(|a| a.get("l2_tx"))
            .and_then(|l| l.get("tx_hash"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };
    Ok((pub_inputs, witness))
}

/// Mock prover hook: hashes public inputs + witness to emit a proof stub.
/// Replace with Stwo/LuminAIR prover when ready.
pub fn mock_prove(pub_inputs: &PublicInputs, witness: &Witness) -> Proof {
    Proof {
        // Use the actual receipt signature as the “proof” payload to avoid the prior hash stub.
        proof_id: format!("proof_{}", pub_inputs.receipt_hash.trim_start_matches("0x")),
        proof: witness.signature_hex.clone(),
        public_inputs: pub_inputs.clone(),
        witness_summary: WitnessSummary {
            receipt_id: witness.receipt_id.clone(),
            anchor_tx_hash: witness.anchor_tx_hash.clone(),
            canonical_len: witness.canonical_receipt.len(),
        },
        prover: "receipt_sig".into(),
    }
}

/// External prover hook: call command in LUMINAIR_PROVER_CMD with stdin JSON {public_inputs, witness}.
/// The command must return a JSON Proof on stdout.
pub fn external_prove(pub_inputs: &PublicInputs, witness: &Witness) -> Result<Proof, VerifyError> {
    let cmd_str = std::env::var("LUMINAIR_PROVER_CMD")
        .map_err(|_| VerifyError::Prover("LUMINAIR_PROVER_CMD not set".into()))?;
    let mut parts = cmd_str.split_whitespace();
    let bin = parts.next().ok_or_else(|| VerifyError::Prover("empty LUMINAIR_PROVER_CMD".into()))?;
    let args: Vec<&str> = parts.collect();
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| VerifyError::Prover(e.to_string()))?;
    if let Some(stdin) = child.stdin.as_mut() {
        let payload = serde_json::json!({ "public_inputs": pub_inputs, "witness": witness });
        stdin
            .write_all(serde_json::to_string(&payload).unwrap().as_bytes())
            .map_err(|e| VerifyError::Prover(e.to_string()))?;
    }
    let output = child.wait_with_output().map_err(|e| VerifyError::Prover(e.to_string()))?;
    if !output.status.success() {
        return Err(VerifyError::Prover(format!(
            "prover failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let proof: Proof = serde_json::from_slice(&output.stdout)
        .map_err(|e| VerifyError::Prover(e.to_string()))?;
    Ok(proof)
}
