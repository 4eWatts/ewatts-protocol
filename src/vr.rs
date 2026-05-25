/// VR = avg_eff * block_time * window / (360M * total_ewatts) in VR_PRECISION units
pub fn compute_vr_int(
    avg_eff: u64,
    total_ewatts: u64,
    window_blocks: u64,
    block_time_secs: u64,
) -> u64 {
    if total_ewatts == 0 || window_blocks == 0 || avg_eff == 0 {
        return 0;
    }
    let total_secs = window_blocks.saturating_mul(block_time_secs);
    let numerator = avg_eff
        .saturating_mul(total_secs)
        .saturating_mul(crate::constants::VR_PRECISION);
    let denominator = 360_000_000u64.saturating_mul(total_ewatts);
    if denominator == 0 { return 0; }
    numerator / denominator
}

pub fn format_vr_int(vr: u64) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_vr_basic_int() {
        // avg_eff=100e9 (100 GB/s * COMMIT_PRECISION), total=100e9 (emission),
        // 1 block, 600s → VR should be non-zero
        let vr = compute_vr_int(100_000_000_000, 100_000_000_000, 1, 600);
        assert!(vr > 0, "VR should be positive, got {}", vr);
    }
    #[test]
    fn test_vr_zero() {
        let vr = compute_vr_int(0, 100_000_000_000, 1000, 600);
        assert_eq!(vr, 0, "Zero avg_eff → zero VR");
    }
    #[test]
    fn test_vr_doubles() {
        let a = compute_vr_int(100_000_000_000, 100_000_000_000, 1, 600);
        let b = compute_vr_int(200_000_000_000, 100_000_000_000, 1, 600);
        assert!(b >= a, "Double avg_eff → at least same VR (got a={}, b={})", a, b);
    }
    #[test]
    fn test_format_vr_int_basic() {
        let s = format_vr_int(1_000_000); // 1.0 kWh/Ewatt
        assert!(s.contains("kWh"), "Format should show kWh, got: {}", s);
    }
    #[test]
    fn test_format_vr_zero() {
        assert_eq!(format_vr_int(0), "0.000 kWh/Ewatt");
    }
}
