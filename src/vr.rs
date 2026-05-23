use crate::constants;

// ─── Integer math versions (f64→u64 migration) ───────────────────────
// All effective GB/s values in COMMIT_PRECISION (1e9) units.
// Final VR in VR_PRECISION (1e6) units = milli-VR.

/// Integer version of compute_vr.
/// avg_eff: average effective commitment in COMMIT_PRECISION units
/// total_ewatts: total Ewatt mined in EMISSION_PRECISION units
/// window_blocks: number of blocks in window
/// block_time_secs: target block time in seconds
/// Returns VR in VR_PRECISION units (1e6 = 1.0 kWh/Ewatt)
pub fn compute_vr_int(
    avg_eff: u64,
    total_ewatts: u64,
    window_blocks: u64,
    block_time_secs: u64,
) -> u64 {
    if total_ewatts == 0 || window_blocks == 0 || avg_eff == 0 {
        return 0;
    }
    // total_gb = (avg_effective_gbps * total_secs) / 8
    // total_joules = total_gb * J_PER_GB
    // total_kwh = total_joules / J_PER_KWH
    // vr = total_kwh / total_ewatts_mined
    //
    // J_PER_GB = 0.08 = 8/100, J_PER_KWH = 3,600,000
    //
    // vr = (avg_eff * block_time * window / 8 * 0.08 / 3,600,000) / total_ewatts
    //   = (avg_eff * block_time * window * 0.01) / (3,600,000 * total_ewatts)
    //   = (avg_eff * block_time * window) / (360,000,000 * total_ewatts)
    //
    // Precision: multiply by VR_PRECISION first
    let total_secs = window_blocks.saturating_mul(block_time_secs);
    let numerator = avg_eff
        .saturating_mul(total_secs)
        .saturating_mul(crate::constants::VR_PRECISION);
    let denominator = 360_000_000u64.saturating_mul(total_ewatts);
    if denominator == 0 { return 0; }
    numerator / denominator
}

pub fn format_vr_int(vr: u64) -> String {
    // vr is in VR_PRECISION units (1e6 = 1.0 kWh/Ewatt)
    if vr == 0 { return "0.000 kWh/Ewatt".to_string(); }
    let kwh = vr as f64 / crate::constants::VR_PRECISION as f64;
    if kwh < 1e-6 {
        format!("{:.3} uWh/Ewatt", kwh * 1e9)
    } else if kwh < 1e-3 {
        format!("{:.3} Wh/Ewatt", kwh * 1000.0)
    } else {
        format!("{:.6} kWh/Ewatt", kwh)
    }
}

pub struct VrResult {
    pub block_number: u64,
    pub vr_kwh_per_ewatt: f64,
    pub total_energy_joules: f64,
    pub total_ewatts_mined: f64,
    pub window_blocks: u64,
}

pub fn compute_vr(
    avg_effective_gbps: f64,
    total_ewatts_mined: f64,
    window_blocks: u64,
    block_time_secs: u64,
) -> VrResult {
    if total_ewatts_mined <= 0.0
        || window_blocks == 0
        || !avg_effective_gbps.is_finite()
        || avg_effective_gbps <= 0.0
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
    // Convert Gbps to GB/s (divide by 8) and compute joules
    let total_gb = (avg_effective_gbps * total_secs) / 8.0;
    let total_joules = total_gb * constants::J_PER_GB;
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

pub fn compute_vr_series(eff: &[f64], emit: &[f64], window: u64, bt: u64) -> Vec<VrResult> {
    let n = eff.len();
    if n < window as usize || emit.len() != n {
        return vec![];
    }
    let mut s = Vec::with_capacity(n - window as usize);
    for i in (window as usize)..n {
        let avg = eff[i - window as usize..i].iter().sum::<f64>() / window as f64;
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
        let v = compute_vr(100., 100_000., 1000, 600);
        assert!(v.vr_kwh_per_ewatt > 0.);
    }
    #[test]
    fn test_vr_zero() {
        let v = compute_vr(100., 0., 1000, 600);
        assert_eq!(v.vr_kwh_per_ewatt, 0.);
    }
    #[test]
    fn test_settlement() {
        assert!((estimate_settlement(100., 0.001) - 100_000.).abs() < 1.);
    }
    #[test]
    fn test_vr_series() {
        let s = compute_vr_series(&[100.; 2000], &[100.; 2000], 1000, 600);
        assert_eq!(s.len(), 1000);
    }
    #[test]
    fn test_vr_doubles() {
        let a = compute_vr(100., 100_000., 1000, 600);
        let b = compute_vr(200., 100_000., 1000, 600);
        assert!((b.vr_kwh_per_ewatt / a.vr_kwh_per_ewatt - 2.).abs() < 1e-6);
    }

    #[test]
    fn test_vr_gbps_to_gb_conversion() {
        // 100 Gbps for 600s = 60,000 Gb = 7,500 GB at 0.08 J/GB = 600 J
        let v = compute_vr(100., 100_000., 1, 600);
        assert!(
            (v.total_energy_joules - 600.0).abs() < 1.0,
            "expected ~600J, got {}",
            v.total_energy_joules
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
