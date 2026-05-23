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

// ─── Integer math versions (f64→u64 migration) ───────────────────────
// All values in COMMIT_PRECISION (1e9) units unless otherwise noted.

/// Integer version of compute_efficiency.
/// work_mbytes: work in megabytes (u64)
/// bw_mgbps: bandwidth in milli-GB/s (1 GB/s = 1000 mGB/s)
/// time_ms: time in milliseconds
/// Returns efficiency in EFF_PRECISION units (1.0 = 1_000_000)
pub fn compute_efficiency_int(work_mbytes: u64, bw_mgbps: u64, time_ms: u64) -> u64 {
    if bw_mgbps == 0 || time_ms == 0 {
        return 0;
    }
    // efficiency = work / (bw * time)
    // work_mbytes / ((bw_mgbps / 1000.0) * (time_ms / 1000.0))
    // = work_mbytes / (bw_mgbps * time_ms / 1_000_000)
    // = (work_mbytes * 1_000_000 * EFF_PRECISION) / (bw_mgbps * time_ms * EFF_PRECISION / EFF_PRECISION)
    // Simplified: (work_mbytes * 1_000_000 * eff_precision) / (bw_mgbps * time_ms)
    // To avoid overflow: (work_mbytes * 1_000_000 / bw_mgbps) * eff_precision / time_ms
    let numerator = work_mbytes.saturating_mul(1_000_000);
    let ratio = numerator / bw_mgbps.max(1);
    let eff = ratio.saturating_mul(crate::constants::EFF_PRECISION) / time_ms.max(1);
    eff.min(crate::constants::EFF_PRECISION * 2) // cap at 2.0
}

/// Integer version of effective_commitment.
/// bandwidth: bandwidth in COMMIT_PRECISION units (1 GB/s = 1e9)
/// efficiency: efficiency in EFF_PRECISION units (1.0 = 1e6)
/// Returns effective commitment in COMMIT_PRECISION units.
pub fn effective_commitment_int(bandwidth: u64, efficiency: u64) -> u64 {
    let cap = crate::constants::EFFICIENCY_CAP_THRESHOLD_INT; // 1.3 * 1e6
    let penalty = crate::constants::EFFICIENCY_PENALTY_THRESHOLD_INT; // 0.7 * 1e6
    
    if efficiency < penalty {
        // d * e: bandwidth * efficiency / EFF_PRECISION
        bandwidth.saturating_mul(efficiency) / crate::constants::EFF_PRECISION
    } else if efficiency > cap {
        // d * 1.3: bandwidth * 1_300_000 / 1_000_000
        bandwidth.saturating_mul(cap) / crate::constants::EFF_PRECISION
    } else {
        bandwidth
    }
}

pub fn compute_efficiency(w: f64, d: f64, t: f64) -> f64 {
    if !w.is_finite() || !d.is_finite() || !t.is_finite() || d <= 0.0 || t <= 0.0 {
        0.0
    } else {
        w / (d * t)
    }
}

pub fn effective_commitment(d: f64, e: f64) -> f64 {
    if !d.is_finite() || !e.is_finite() {
        return 0.0;
    }
    if e < 0.7 {
        d * e
    } else if e > 1.3 {
        d * 1.3
    } else {
        d
    }
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Less));
    let l = s.len();
    if l == 0 {
        0.0
    } else if l % 2 == 0 {
        (s[l / 2 - 1] + s[l / 2]) / 2.0
    } else {
        s[l / 2]
    }
}

pub fn min_commitment_int(r: &[u64]) -> u64 {
    // Minimum bandwidth: 1.0 GB/s = 1000 mGB/s
    // Rolling minimum: 0.1 * median(r)
    if r.is_empty() {
        1000
    } else {
        let mut s = r.to_vec();
        s.sort();
        let med = s[s.len() / 2];
        1000.max(med / 10) // 0.1 * median
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
    fn test_eff() {
        assert!((compute_efficiency(100., 100., 1.) - 1.).abs() < 1e-6);
    }
    #[test]
    fn test_penalty() {
        assert!((effective_commitment(100., 0.5) - 50.).abs() < 1e-6);
    }
    #[test]
    fn test_cap() {
        assert!((effective_commitment(100., 2.0) - 130.).abs() < 1e-6);
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
    fn test_efficiency_nan_guard() {
        assert_eq!(compute_efficiency(f64::NAN, 100.0, 1.0), 0.0, "NaN w");
        assert_eq!(compute_efficiency(100.0, f64::NAN, 1.0), 0.0, "NaN d");
        assert_eq!(compute_efficiency(100.0, 100.0, f64::NAN), 0.0, "NaN t");
        assert_eq!(
            compute_efficiency(f64::NAN, f64::NAN, f64::NAN),
            0.0,
            "all NaN"
        );
    }

    #[test]
    fn test_effective_commitment_nan_guard() {
        assert_eq!(effective_commitment(f64::NAN, 1.0), 0.0, "NaN d");
        assert_eq!(effective_commitment(100.0, f64::NAN), 0.0, "NaN e");
    }

    #[test]
    fn test_median_nan_guard() {
        // With NaN in input, median should not panic and return a finite value
        let v = vec![1.0, f64::NAN, 3.0];
        let m = median(&v);
        assert!(m.is_finite(), "median with NaN should be finite, got {}", m);
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
