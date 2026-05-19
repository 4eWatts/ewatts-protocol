use crate::constants;

pub fn adjust_difficulty(current: u64, actual_accesses: f64, target_accesses: f64) -> u64 {
    if actual_accesses <= 0.0 || target_accesses <= 0.0 {
        return current;
    }
    let ratio = (target_accesses / actual_accesses).clamp(
        constants::DIFFICULTY_BOUND_MIN,
        constants::DIFFICULTY_BOUND_MAX,
    );
    (current as f64 * ratio).max(1.0) as u64
}

pub fn average_block_time(timestamps: &[u64]) -> f64 {
    if timestamps.len() < 2 {
        return constants::TARGET_BLOCK_TIME_SECS as f64;
    }
    let diffs: Vec<f64> = timestamps
        .windows(2)
        .map(|w| w[1].saturating_sub(w[0]) as f64)
        .filter(|&d| d > 0.0 && d < 3600.0)
        .collect();
    if diffs.is_empty() {
        // All diffs filtered; assume network is struggling, ease slightly
        return constants::TARGET_BLOCK_TIME_SECS as f64 * 1.5;
    }
    // Use median instead of mean for robustness against timestamp manipulation
    let mut sorted = diffs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_adjust() {
        assert_eq!(adjust_difficulty(1000, 1000., 1000.), 1000);
    }
    #[test]
    fn test_adjust_half() {
        assert_eq!(adjust_difficulty(1000, 2000., 1000.), 500);
    }
    #[test]
    fn test_adjust_clamp() {
        assert_eq!(adjust_difficulty(1000, 1e10, 1000.), 500);
    }
    #[test]
    fn test_avg_time() {
        assert!((average_block_time(&[100, 700, 1300, 1900]) - 600.).abs() < 1.);
    }
}
