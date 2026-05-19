#[cfg(test)]
use ed25519_dalek::SigningKey;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

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

pub fn min_commitment(r: &[f64]) -> f64 {
    if r.is_empty() {
        1.0
    } else {
        1.0f64.max(0.1 * median(r))
    }
}

pub fn commit_msg(commit: &Commitment) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(&commit.miner_id);
    msg.extend_from_slice(&commit.bandwidth_gbps.to_le_bytes());
    msg.extend_from_slice(&commit.block_number.to_le_bytes());
    msg.extend_from_slice(&commit.work_gb.to_le_bytes());
    msg.extend_from_slice(&commit.time_seconds.to_le_bytes());
    msg
}

pub fn validate_commitment(c: &Commitment, r: &[f64]) -> Result<(), String> {
    if c.bandwidth_gbps < 1.0 {
        return Err("abaixo do minimo".into());
    }
    if c.bandwidth_gbps < min_commitment(r) {
        return Err("abaixo do minimo rolling".into());
    }
    if c.signature.len() != 64 {
        return Err("assinatura invalida".into());
    }
    let e = compute_efficiency(c.work_gb, c.bandwidth_gbps, c.time_seconds);
    if e <= 0.0 {
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
            bandwidth_gbps: 100.,
            block_number: 1,
            work_gb: 100.,
            time_seconds: 1.,
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
            bandwidth_gbps: 100.,
            block_number: 1,
            work_gb: 100.,
            time_seconds: 1.,
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
        assert!((min_commitment(&[]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_validate_commitment_short_sig() {
        let c = Commitment {
            miner_id: [0; 32],
            bandwidth_gbps: 100.,
            block_number: 1,
            work_gb: 100.,
            time_seconds: 1.,
            signature: vec![0; 32], // too short (should be 64)
        };
        assert!(
            validate_commitment(&c, &[]).is_err(),
            "short sig should be rejected"
        );
    }
}
