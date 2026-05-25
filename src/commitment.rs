#[cfg(test)]
use ed25519_dalek::SigningKey;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commitment {
    pub miner_id: [u8; 32],
    pub bandwidth_mgbps: u64,   // milli-GB/s (1 GB/s = 1000 mGB/s)
    pub block_number: u64,
    pub work_mbytes: u64,       // mega-bytes (1 GB = 1000 MB)
    pub time_ms: u64,           // milliseconds
    pub signature: Vec<u8>,
}

/// eff = work / (bw * time) in EFF_PRECISION units
pub fn compute_efficiency_int(work_mbytes: u64, bw_mgbps: u64, time_ms: u64) -> u64 {
    if bw_mgbps == 0 || time_ms == 0 {
        return 0;
    }
    let numerator = work_mbytes.saturating_mul(1_000_000);
    let ratio = numerator / bw_mgbps.max(1);
    let eff = ratio.saturating_mul(crate::constants::EFF_PRECISION) / time_ms.max(1);
    eff.min(crate::constants::EFF_PRECISION * 2)
}

/// Effective commitment = bandwidth adjusted by efficiency, in COMMIT_PRECISION units
pub fn effective_commitment_int(bandwidth: u64, efficiency: u64) -> u64 {
    let cap = crate::constants::EFFICIENCY_CAP_THRESHOLD_INT;
    let penalty = crate::constants::EFFICIENCY_PENALTY_THRESHOLD_INT;
    if efficiency < penalty {
        bandwidth.saturating_mul(efficiency) / crate::constants::EFF_PRECISION
    } else if efficiency > cap {
        bandwidth.saturating_mul(cap) / crate::constants::EFF_PRECISION
    } else {
        bandwidth
    }
}

pub fn min_commitment_int(r: &[u64]) -> u64 {
    if r.is_empty() {
        1000
    } else {
        let mut s = r.to_vec();
        s.sort();
        let med = s[s.len() / 2];
        1000.max(med / 10)
    }
}

pub fn commit_msg(commit: &Commitment) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(&commit.miner_id);
    msg.extend_from_slice(&commit.bandwidth_mgbps.to_le_bytes());
    msg.extend_from_slice(&commit.block_number.to_le_bytes());
    msg.extend_from_slice(&commit.work_mbytes.to_le_bytes());
    msg.extend_from_slice(&commit.time_ms.to_le_bytes());
    msg
}

pub fn validate_commitment(c: &Commitment, r: &[u64]) -> Result<(), String> {
    if c.bandwidth_mgbps < 1000 {
        return Err("abaixo do minimo".into());
    }
    if c.bandwidth_mgbps < min_commitment_int(r) {
        return Err("abaixo do minimo rolling".into());
    }
    if c.signature.len() != 64 {
        return Err("assinatura invalida".into());
    }
    let e = compute_efficiency_int(c.work_mbytes, c.bandwidth_mgbps, c.time_ms);
    if e == 0 {
        return Err("eficiencia zero".into());
    }
    let pubkey = VerifyingKey::from_bytes(&c.miner_id).map_err(|_| "chave invalida")?;
    let sig = Signature::from_slice(&c.signature).map_err(|_| "assinatura invalida")?;
    let msg = commit_msg(c);
    pubkey
        .verify(&msg, &sig)
        .map_err(|_| "assinatura nao confere")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    use rand::RngCore;
    fn make_key() -> SigningKey {
        let mut b = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut b);
        SigningKey::from_bytes(&b)
    }
    #[test]
    fn test_eff_int() {
        let e = compute_efficiency_int(100_000, 100_000, 1_000);
        assert!(e >= crate::constants::EFF_PRECISION - 1000, "eff ~1.0, got {}", e);
    }
    #[test]
    fn test_penalty_int() {
        let ce = effective_commitment_int(1_000_000_000, 500_000);
        assert_eq!(ce, 500_000_000);
    }
    #[test]
    fn test_cap_int() {
        let ce = effective_commitment_int(1_000_000_000, 2_000_000);
        assert_eq!(ce, 1_300_000_000);
    }
    #[test]
    fn test_sign() {
        let sk = make_key();
        let pk = sk.verifying_key().to_bytes();
        let mut c = Commitment {
            miner_id: pk,
            bandwidth_mgbps: 100_000,
            block_number: 1,
            work_mbytes: 100_000,
            time_ms: 1_000,
            signature: vec![],
        };
        let msg = commit_msg(&c);
        c.signature = sk.sign(&msg).to_bytes().to_vec();
        assert!(validate_commitment(&c, &[]).is_ok());
    }
    #[test]
    fn test_bad_sig() {
        let c = Commitment {
            miner_id: [0; 32],
            bandwidth_mgbps: 100_000,
            block_number: 1,
            work_mbytes: 100_000,
            time_ms: 1_000,
            signature: vec![0; 64],
        };
        assert!(validate_commitment(&c, &[]).is_err());
    }

    #[test]
    fn test_min_commitment_empty() {
        assert_eq!(min_commitment_int(&[]), 1000);
    }

    #[test]
    fn test_validate_commitment_short_sig() {
        let c = Commitment {
            miner_id: [0; 32],
            bandwidth_mgbps: 100_000,
            block_number: 1,
            work_mbytes: 100_000,
            time_ms: 1_000,
            signature: vec![0; 32], // too short (should be 64)
        };
        assert!(
            validate_commitment(&c, &[]).is_err(),
            "short sig should be rejected"
        );
    }
}
