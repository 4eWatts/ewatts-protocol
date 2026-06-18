use crate::constants;

pub struct VrResult {
    pub block_number: u64,
    pub vr_kwh_per_ewatt: f64,
    pub total_energy_joules: f64,
    pub total_ewatts_mined: f64,
    pub window_blocks: u64,
}

/// Compute VR usando AOPS (Access OPerations per Second) como métrica primária.
/// A energia é derivada do número de acessos × J_PER_ACCESS (wall power realista).
pub fn compute_vr(
    avg_effective_aops: f64,     // access operations per second
    total_ewatts_mined: f64,
    window_blocks: u64,
    block_time_secs: u64,
) -> VrResult {
    if total_ewatts_mined <= 0.0
        || window_blocks == 0
        || !avg_effective_aops.is_finite()
        || avg_effective_aops <= 0.0
    {
        return VrResult {
            block_number: 0,
            vr_kwh_per_ewatt: 0.0,
            total_energy_joules: 0.0,
            total_ewatts_mined: if total_ewatts_mined > 0.0 {
                total_ewatts_mined
            } else {
                0.0
            },
            window_blocks,
        };
    }
    let total_secs = window_blocks as f64 * block_time_secs as f64;
    // Energia = total_acessos × joules por acesso (wall power)
    let total_accesses = avg_effective_aops * total_secs;
    let total_joules = total_accesses * constants::J_PER_ACCESS;
    let total_kwh = total_joules / constants::J_PER_KWH;
    VrResult {
        block_number: 0,
        vr_kwh_per_ewatt: total_kwh / total_ewatts_mined,
        total_energy_joules: total_joules,
        total_ewatts_mined,
        window_blocks,
    }
}

pub fn estimate_settlement(kwh_amount: f64, vr: f64) -> f64 {
    if vr <= 0.0 {
        0.0
    } else {
        kwh_amount / vr
    }
}

pub fn format_vr(vr: f64) -> String {
    if !vr.is_finite() {
        return "0.000 kWh/Ewatt".to_string();
    }
    if vr < 1e-6 {
        format!("{:.3} uWh/Ewatt", vr * 1e9)
    } else if vr < 1e-3 {
        format!("{:.3} Wh/Ewatt", vr * 1000.0)
    } else {
        format!("{:.6} kWh/Ewatt", vr)
    }
}

pub fn compute_vr_series(aops: &[f64], emit: &[f64], window: u64, bt: u64) -> Vec<VrResult> {
    let n = aops.len();
    if n < window as usize || emit.len() != n {
        return vec![];
    }
    let mut s = Vec::with_capacity(n - window as usize);
    for i in (window as usize)..n {
        let avg = aops[i - window as usize..i].iter().sum::<f64>() / window as f64;
        let total: f64 = emit[i - window as usize..i].iter().sum();
        let mut vr = compute_vr(avg, total, window, bt);
        vr.block_number = i as u64;
        s.push(vr);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_vr_basic() {
        let v = compute_vr(20_000_000., 100_000., 1000, 600);
        assert!(v.vr_kwh_per_ewatt > 0.);
    }
    #[test]
    fn test_vr_zero() {
        let v = compute_vr(20_000_000., 0., 1000, 600);
        assert_eq!(v.vr_kwh_per_ewatt, 0.);
    }
    #[test]
    fn test_settlement() {
        assert!((estimate_settlement(100., 0.001) - 100_000.).abs() < 1.);
    }
    #[test]
    fn test_vr_series() {
        let s = compute_vr_series(&[20_000_000.; 2000], &[100.; 2000], 1000, 600);
        assert_eq!(s.len(), 1000);
    }
    #[test]
    fn test_vr_doubles() {
        let a = compute_vr(20_000_000., 100_000., 1000, 600);
        let b = compute_vr(40_000_000., 100_000., 1000, 600);
        assert!((b.vr_kwh_per_ewatt / a.vr_kwh_per_ewatt - 2.).abs() < 1e-6);
    }

    #[test]
    fn test_vr_aops_to_joules() {
        // 25M ops/s por 600s = 15B ops × 3.75e-6 J/op = 56,250 J
        let v = compute_vr(25_000_000., 100_000., 1, 600);
        let expected = 25_000_000.0 * 600.0 * 3.75e-6; // 56,250
        assert!(
            (v.total_energy_joules - expected).abs() < 0.1,
            "expected ~{}J, got {}",
            expected, v.total_energy_joules
        );
    }

    #[test]
    fn test_vr_nan_guard() {
        let v = compute_vr(f64::NAN, 100_000., 1000, 600);
        assert_eq!(v.vr_kwh_per_ewatt, 0.0, "NaN bandwidth should yield 0 VR");
    }

    #[test]
    fn test_format_vr_nan() {
        assert_eq!(format_vr(f64::NAN), "0.000 kWh/Ewatt");
    }
}
