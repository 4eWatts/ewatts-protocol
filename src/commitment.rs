use crate::constants;

#[cfg(test)]
use ed25519_dalek::SigningKey;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commitment {
    pub miner_id: [u8; 32],
    /// Access operations per second (AOPS) — métrica primária.
    /// DDR moderna faz ~20-25M random accesses/s.
    pub access_ops_per_sec: f64,
    pub block_number: u64,
    /// Total de acessos realizados (access ops), substitui work_gb.
    pub total_access_ops: f64,
    pub time_seconds: f64,
    pub signature: Vec<u8>,
}

impl Commitment {
    /// Deriva bandwidth equivalente de AOPS para compatibilidade.
    /// Cada acesso = DAG_ELEMENT_SIZE (64) bytes.
    pub fn bandwidth_gbps(&self) -> f64 {
        self.access_ops_per_sec * 64.0 / 1_000_000_000.0
    }
    pub fn work_gb(&self) -> f64 {
        self.total_access_ops * 64.0 / 1_000_000_000.0
    }
}

pub fn compute_efficiency(w: f64, d: f64, t: f64) -> f64 {
    if !w.is_finite() || !d.is_finite() || !t.is_finite() || d <= 0.0 || t <= 0.0 {
        0.0
    } else {
        w / (d * t)
    }
}

/// Versão AOPS: efficiency = total_access_ops / (declared_ops_per_sec × time)
pub fn compute_efficiency_aops(total_ops: f64, declared_ops_per_sec: f64, time_secs: f64) -> f64 {
    if !total_ops.is_finite() || !declared_ops_per_sec.is_finite() || !time_secs.is_finite()
        || declared_ops_per_sec <= 0.0 || time_secs <= 0.0
    {
        0.0
    } else {
        total_ops / (declared_ops_per_sec * time_secs)
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
        constants::MIN_COMMIT_AOPS
    } else {
        constants::MIN_COMMIT_AOPS.max(0.1 * median(r))
    }
}

pub fn commit_msg(commit: &Commitment) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(&commit.miner_id);
    msg.extend_from_slice(&commit.access_ops_per_sec.to_le_bytes());
    msg.extend_from_slice(&commit.block_number.to_le_bytes());
    msg.extend_from_slice(&commit.total_access_ops.to_le_bytes());
    msg.extend_from_slice(&commit.time_seconds.to_le_bytes());
    msg
}

pub fn validate_commitment(c: &Commitment, r: &[f64]) -> Result<(), String> {
    // Primary check: AOPS minimum
    if c.access_ops_per_sec < constants::MIN_COMMIT_AOPS {
        return Err(format!(
            "abaixo do minimo AOPS: {:.0} < {:.0}",
            c.access_ops_per_sec, constants::MIN_COMMIT_AOPS
        ));
    }
    if c.access_ops_per_sec < min_commitment(r) {
        return Err("abaixo do minimo rolling".into());
    }
    if c.signature.len() != 64 {
        return Err("assinatura invalida".into());
    }
    let e = compute_efficiency_aops(c.total_access_ops, c.access_ops_per_sec, c.time_seconds);
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
    fn test_eff_aops() {
        // 25M ops em 1s com declarados 25M/s = eficiencia 1.0
        assert!((compute_efficiency_aops(25_000_000., 25_000_000., 1.) - 1.).abs() < 1e-6);
        // 10M ops em 1s com declarados 25M/s = eficiencia 0.4
        assert!((compute_efficiency_aops(10_000_000., 25_000_000., 1.) - 0.4).abs() < 1e-6);
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
    fn test_bandwidth_derived() {
        // 25M ops/s × 64 bytes = 1,600,000,000 bytes/s = 1.6 GB/s
        let c = Commitment {
            miner_id: [0; 32],
            access_ops_per_sec: 25_000_000.,
            block_number: 0,
            total_access_ops: 0.,
            time_seconds: 1.,
            signature: vec![],
        };
        let bw = c.bandwidth_gbps();
        assert!((bw - 1.6).abs() < 0.01, "expected ~1.6 GB/s, got {}", bw);
    }
    #[test]
    fn test_sign() {
        let sk = make_key();
        let pk = sk.verifying_key().to_bytes();
        let mut c = Commitment {
            miner_id: pk,
            access_ops_per_sec: 25_000_000.,
            block_number: 1,
            total_access_ops: 25_000_000.,
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
            access_ops_per_sec: 25_000_000.,
            block_number: 1,
            total_access_ops: 25_000_000.,
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
        assert!((min_commitment(&[]) - constants::MIN_COMMIT_AOPS).abs() < 1e-6);
    }

    #[test]
    fn test_validate_commitment_short_sig() {
        let c = Commitment {
            miner_id: [0; 32],
            access_ops_per_sec: 25_000_000.,
            block_number: 1,
            total_access_ops: 25_000_000.,
            time_seconds: 1.,
            signature: vec![0; 32], // too short (should be 64)
        };
        assert!(
            validate_commitment(&c, &[]).is_err(),
            "short sig should be rejected"
        );
    }
}
