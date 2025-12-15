use std::io::{self, Read};
use receipt_verifier::{PublicInputs, Witness, mock_prove, external_prove};
use serde::Deserialize;

#[derive(Deserialize)]
struct ProverInput {
    public_inputs: PublicInputs,
    witness: Witness,
}

fn main() -> anyhow::Result<()> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let input: ProverInput = serde_json::from_str(&buf)?;
    // Try external prover first if set, then fall back to receipt_sig.
    let proof = external_prove(&input.public_inputs, &input.witness)
        .unwrap_or_else(|_| mock_prove(&input.public_inputs, &input.witness));
    println!("{}", serde_json::to_string(&proof)?);
    Ok(())
}
