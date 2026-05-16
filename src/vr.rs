use crate::constants;

pub struct VrResult {
    pub block_number: u64,
    pub vr_kwh_per_ewatt: f64,
    pub total_energy_joules: f64,
    pub total_ewatts_mined: f64,
    pub window_blocks: u64,
}

pub fn compute_vr(avg_effective_gbps: f64, total_ewatts_mined: f64, window_blocks: u64, block_time_secs: u64) -> VrResult {
    if total_ewatts_mined <= 0.0 || window_blocks == 0 {
        return VrResult { block_number: 0, vr_kwh_per_ewatt: 0.0, total_energy_joules: 0.0, total_ewatts_mined: 0.0, window_blocks };
    }
    let total_secs = window_blocks as f64 * block_time_secs as f64;
    let total_joules = avg_effective_gbps * total_secs * constants::J_PER_GB;
    let total_kwh = total_joules / constants::J_PER_KWH;
    VrResult { block_number: 0, vr_kwh_per_ewatt: total_kwh / total_ewatts_mined, total_energy_joules: total_joules, total_ewatts_mined, window_blocks }
}

pub fn estimate_settlement(kwh_amount: f64, vr: f64) -> f64 {
    if vr <= 0.0 { 0.0 } else { kwh_amount / vr }
}

pub fn format_vr(vr: f64) -> String {
    if vr < 1e-6 { format!("{:.3} uWh/Ewatt", vr * 1e9) }
    else if vr < 1e-3 { format!("{:.3} Wh/Ewatt", vr * 1000.0) }
    else { format!("{:.6} kWh/Ewatt", vr) }
}

pub fn compute_vr_series(eff: &[f64], emit: &[f64], window: u64, bt: u64) -> Vec<VrResult> {
    let n = eff.len();
    if n < window as usize { return vec![]; }
    let mut s = Vec::with_capacity(n - window as usize);
    for i in (window as usize)..n {
        let avg = eff[i-window as usize..i].iter().sum::<f64>() / window as f64;
        let total: f64 = emit[i-window as usize..i].iter().sum();
        let mut vr = compute_vr(avg, total, window, bt);
        vr.block_number = i as u64;
        s.push(vr);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_vr_basic() { let v = compute_vr(100., 100_000., 1000, 600); assert!(v.vr_kwh_per_ewatt > 0.); }
    #[test] fn test_vr_zero() { let v = compute_vr(100., 0., 1000, 600); assert_eq!(v.vr_kwh_per_ewatt, 0.); }
    #[test] fn test_settlement() { assert!((estimate_settlement(100., 0.001) - 100_000.).abs() < 1.); }
    #[test] fn test_vr_series() { let s = compute_vr_series(&[100.;2000], &[100.;2000], 1000, 600); assert_eq!(s.len(), 1000); }
    #[test] fn test_vr_doubles() {
        let a = compute_vr(100., 100_000., 1000, 600);
        let b = compute_vr(200., 100_000., 1000, 600);
        assert!((b.vr_kwh_per_ewatt / a.vr_kwh_per_ewatt - 2.).abs() < 1e-6);
    }
}
