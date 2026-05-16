use ed25519_dalek::{Verifier, VerifyingKey, Signature};
use serde::{Serialize, Deserialize};
use crate::constants;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commitment {
    pub miner_id: [u8; 32],
    pub bandwidth_gbps: f64,
    pub block_number: u64,
    pub work_gb: f64,
    pub time_seconds: f64,
    pub signature: Vec<u8>,
}

pub fn compute_efficiency(w: f64, d: f64, t: f64) -> f64 {
    if d <= 0.0 || t <= 0.0 { 0.0 } else { w / (d * t) }
}

pub fn effective_commitment(d: f64, e: f64) -> f64 {
    if e < 0.7 { d * e } else if e > 1.3 { d * 1.3 } else { d }
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let l = s.len();
    if l == 0 { 0.0 } else if l % 2 == 0 { (s[l/2-1] + s[l/2]) / 2.0 } else { s[l/2] }
}

pub fn min_commitment(recent_effective: &[f64]) -> f64 {
    if recent_effective.is_empty() { return 1.0; }
    1.0f64.max(0.1 * median(recent_effective))
}

/// Sign the commitment data using a keypair bytes (seed).
pub fn sign_commitment(seed: &[u8; 32], commit: &Commitment) -> Vec<u8> {
    use ed25519_dalek::Signer;
    let keypair = ed25519_dalek::Keypair::from_bytes(&[seed.as_slice(), &commit.miner_id].concat()).unwrap();
    let msg = commit_msg(&commit);
    keypair.sign(&msg).to_bytes().to_vec()
}

/// The message to sign: all fields except signature.
fn commit_msg(commit: &Commitment) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(&commit.miner_id);
    msg.extend_from_slice(&commit.bandwidth_gbps.to_le_bytes());
    msg.extend_from_slice(&commit.block_number.to_le_bytes());
    msg.extend_from_slice(&commit.work_gb.to_le_bytes());
    msg.extend_from_slice(&commit.time_seconds.to_le_bytes());
    msg
}

pub fn validate_commitment(c: &Commitment, r: &[f64]) -> Result<(), String> {
    if c.bandwidth_gbps < 1.0 { return Err("abaixo do minimo".into()); }
    if c.bandwidth_gbps < min_commitment(r) { return Err("abaixo do minimo rolling".into()); }
    let e = compute_efficiency(c.work_gb, c.bandwidth_gbps, c.time_seconds);
    if e <= 0.0 { return Err("eficiencia zero".into()); }
    // Verify Ed25519 signature
    let pubkey = VerifyingKey::from_bytes(&c.miner_id)
        .map_err(|_| "chave publica invalida".to_string())?;
    let sig = Signature::from_slice(&c.signature)
        .map_err(|_| "assinatura invalida".to_string())?;
    let msg = commit_msg(c);
    pubkey.verify(&msg, &sig)
        .map_err(|_| "assinatura nao confere".to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    #[test] fn test_eff() { assert!((compute_efficiency(100.,100.,1.)-1.).abs()<1e-6); }
    #[test] fn test_penalty() { assert!((effective_commitment(100.,0.5)-50.).abs()<1e-6); }
    #[test] fn test_cap() { assert!((effective_commitment(100.,2.0)-130.).abs()<1e-6); }
    #[test] fn test_median() { assert!((median(&[10.,20.,30.,40.])-25.).abs()<1e-6); }
    #[test] fn test_min_commit_effective() {
        assert!((min_commitment(&[10.,20.,30.,40.]) - 2.5).abs() < 1e-6);
    }
    #[test] fn test_sign_and_verify() {
        let mut seed = [0u8; 32]; seed[0] = 0xab;
        let keypair = ed25519_dalek::Keypair::generate(&mut rand::thread_rng());
        let pk: [u8; 32] = keypair.verifying_key().to_bytes();
        let mut c = Commitment {
            miner_id: pk, bandwidth_gbps: 100.0, block_number: 1,
            work_gb: 100.0, time_seconds: 1.0, signature: vec![],
        };
        let msg = commit_msg(&c);
        let sig = keypair.sign(&msg);
        c.signature = sig.to_bytes().to_vec();
        assert!(validate_commitment(&c, &[]).is_ok());
    }
    #[test] fn test_bad_signature_rejected() {
        let mut c = Commitment {
            miner_id: [0u8; 32], bandwidth_gbps: 100.0, block_number: 1,
            work_gb: 100.0, time_seconds: 1.0, signature: vec![0u8; 64],
        };
        assert!(validate_commitment(&c, &[]).is_err());
    }
}
