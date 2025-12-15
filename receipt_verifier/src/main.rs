use clap::Parser;
use hex::FromHex;
use receipt_verifier::{verify_receipt, build_public_and_witness};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    /// Path to receipt JSON
    #[arg(long)]
    receipt: String,
    /// Receipt signature hex (65-byte r||s||v)
    #[arg(long)]
    signature: String,
    /// Gateway address (0x...), or \"auto\" to accept recovered signer
    #[arg(long)]
    gateway: String,
    /// Expected policy hash
    #[arg(long)]
    policy_hash: String,
    /// Expected consent hash
    #[arg(long)]
    consent_hash: String,
    /// Path to write public inputs JSON
    #[arg(long)]
    out_public: Option<PathBuf>,
    /// Path to write witness JSON
    #[arg(long)]
    out_witness: Option<PathBuf>,
    /// Path to write proof JSON (currently the receipt_sig hook; swap to Stwo later)
    #[arg(long)]
    out_proof: Option<PathBuf>,
    /// Generate proof via the configured hook (receipt_sig or external)
    #[arg(long)]
    prove: bool,
    /// Force stub prover even if a real prover is later wired
    #[arg(long, default_value_t = false)]
    stub: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let data = fs::read_to_string(&args.receipt)?;
    let val: serde_json::Value = serde_json::from_str(&data)?;
    let gateway_opt = if args.gateway.eq_ignore_ascii_case("auto") {
        None
    } else {
        Some(args.gateway.clone())
    };

    let expected_gateway_bytes = if let Some(gw) = gateway_opt.as_ref() {
        let mut gateway_bytes = [0u8; 20];
        let clean = receipt_verifier::normalize_hex_even(gw);
        let gb = Vec::from_hex(clean)?;
        let slice: &[u8] = if gb.len() == 32 { &gb[12..] } else { &gb };
        if slice.len() != 20 { anyhow::bail!("gateway must be 20 bytes (or 32 felt)") }
        gateway_bytes.copy_from_slice(slice);
        Some(gateway_bytes)
    } else {
        None
    };

    match verify_receipt(&val, &args.signature, expected_gateway_bytes, &args.policy_hash, &args.consent_hash) {
        Ok((rcpt_hash, addr)) => {
            println!("signature ok, policy/consent ok");
            println!("receipt_hash (sha256 canonical): 0x{}", rcpt_hash);
            println!("recovered address: 0x{}", hex::encode(addr));
            let (pub_inputs, witness) = build_public_and_witness(&val, &args.signature, gateway_opt.as_deref())?;
            println!("public inputs JSON:");
            println!("{}", serde_json::to_string_pretty(&pub_inputs)?);
            if let Some(out) = args.out_public {
                fs::write(&out, serde_json::to_vec_pretty(&pub_inputs)?)?;
                println!("saved public inputs to {}", out.display());
            }
            if let Some(out) = args.out_witness {
                fs::write(&out, serde_json::to_vec_pretty(&witness)?)?;
                println!("saved witness to {}", out.display());
            }
            if args.prove {
                let proof = if !args.stub {
                    receipt_verifier::external_prove(&pub_inputs, &witness).unwrap_or_else(|e| {
                        eprintln!("external prover failed or not set: {e}; falling back to receipt_sig");
                        receipt_verifier::mock_prove(&pub_inputs, &witness)
                    })
                } else {
                    receipt_verifier::mock_prove(&pub_inputs, &witness)
                };
                println!("proof:");
                println!("{}", serde_json::to_string_pretty(&proof)?);
                if let Some(out) = args.out_proof {
                    fs::write(&out, serde_json::to_vec_pretty(&proof)?)?;
                    println!("saved proof to {}", out.display());
                }
            }
        }
        Err(e) => {
            eprintln!("verification failed: {e}");
            std::process::exit(1);
        }
    }
    Ok(())
}
