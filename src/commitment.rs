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

pub fn validate_commitment(c: &Commitment, r: &[f64]) -> Result<(), String> {
    if c.bandwidth_gbps < 1.0 { return Err("abaixo do minimo".into()); }
    if c.bandwidth_gbps < min_commitment(r) { return Err("abaixo do minimo rolling".into()); }
    let e = compute_efficiency(c.work_gb, c.bandwidth_gbps, c.time_seconds);
    if e <= 0.0 { return Err("eficiencia zero".into()); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_eff() { assert!((compute_efficiency(100.,100.,1.)-1.).abs()<1e-6); }
    #[test] fn test_penalty() { assert!((effective_commitment(100.,0.5)-50.).abs()<1e-6); }
    #[test] fn test_cap() { assert!((effective_commitment(100.,2.0)-130.).abs()<1e-6); }
    #[test] fn test_median() { assert!((median(&[10.,20.,30.,40.])-25.).abs()<1e-6); }
    #[test] fn test_min_commit_effective() {
        assert!((min_commitment(&[10.,20.,30.,40.]) - 2.5).abs() < 1e-6);
    }
}
