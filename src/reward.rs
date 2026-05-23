use crate::constants;

// ─── Integer math versions (f64→u64 migration) ───────────────────────
// Emission rates in BASE_EMISSION_UNITS * EMISSION_PRECISION / UNITS_PER_EWATT
// = 100_000_000 * 1_000_000_000 / 1_000_000 = 100_000_000_000
// Effective commits in COMMIT_PRECISION (1e9) units.

/// Integer version of compute_emission_rate.
/// total_eff: total effective commitment in COMMIT_PRECISION units
/// hist_avg: historical average commitment in COMMIT_PRECISION units
/// Returns emission rate in EMISSION_PRECISION units (per-block emission).
pub fn compute_emission_rate_int(total_eff: u64, hist_avg: u64) -> u64 {
    if hist_avg == 0 { return crate::constants::BASE_EMISSION_INT; }
    // rate = BASE_EMISSION_INT * total_eff / hist_avg
    let rate = crate::constants::BASE_EMISSION_INT
        .saturating_mul(total_eff) / hist_avg;
    let floor = crate::constants::BASE_EMISSION_INT
        .saturating_mul(crate::constants::EMISSION_FLOOR_MULTIPLIER_INT) / crate::constants::EMISSION_PRECISION;
    let ceil = crate::constants::BASE_EMISSION_INT
        .saturating_mul(crate::constants::EMISSION_CEILING_MULTIPLIER_INT) / crate::constants::EMISSION_PRECISION;
    rate.clamp(floor, ceil)
}

/// Integer version of apply_ramp_up_cap.
/// rewards: (miner_pk, reward_in_EMISSION_PRECISION_units)
/// Returns burned amount in same precision.
pub fn apply_ramp_up_cap_int(block_number: u64, rewards: &mut Vec<(Vec<u8>, u64)>) -> u64 {
    if block_number >= crate::constants::RAMP_UP_BLOCKS {
        return 0;
    }
    let total: u64 = rewards.iter().map(|(_, r)| r).sum();
    if total == 0 { return 0; }
    let mut burned = 0u64;
    for (_, reward) in rewards.iter_mut() {
        // share = reward / total, compare with RAMP_UP_CAP_INT / CAP_PRECISION
        // equivalent to: reward * CAP_PRECISION > total * RAMP_UP_CAP_INT
        let share_exceeds = reward.saturating_mul(crate::constants::CAP_PRECISION)
            > total.saturating_mul(crate::constants::RAMP_UP_CAP_INT);
        if share_exceeds {
            let max_reward = total.saturating_mul(crate::constants::RAMP_UP_CAP_INT)
                / crate::constants::CAP_PRECISION;
            let excess = reward.saturating_sub(max_reward);
            burned = burned.saturating_add(excess);
            *reward = max_reward;
        }
    }
    burned
}

/// Integer version of compute_block_rewards.
/// All commitment values in COMMIT_PRECISION units.
/// Returns miner rewards in base units (1 Ewatt = 1_000_000 units).
pub fn compute_block_rewards_int(
    block_number: u64,
    commitments: &[(u64, [u8; 32])], // (effective_commit, miner_id)
    emission_rate_int: u64,           // from compute_emission_rate_int
) -> Vec<(Vec<u8>, u64)> {
    let total_eff: u64 = commitments.iter().map(|(c, _)| c).sum();
    if total_eff == 0 { return vec![]; }
    
    let mut rewards: Vec<(Vec<u8>, u64)> = commitments.iter().map(|(c, mid)| {
        // Use u128 intermediate to avoid overflow: r = c * emission_rate_int / total_eff
        let r = if total_eff > 0 {
            let num = (*c as u128).saturating_mul(emission_rate_int as u128);
            (num / total_eff as u128) as u64
        } else {
            0
        };
        (mid.to_vec(), r)
    }).collect();
    
    let _burned = apply_ramp_up_cap_int(block_number, &mut rewards);
    
    // Convert from EMISSION_PRECISION units to base units
    rewards.iter().map(|(pk, r)| {
        let base_units = r.saturating_mul(crate::constants::UNITS_PER_EWATT)
            / crate::constants::EMISSION_PRECISION;
        (pk.clone(), base_units)
    }).collect()
}

/// Convert Ewatt (f64) to base units with rounding.
fn ewatt_to_units(ewatt: f64) -> u64 {
    (ewatt * constants::UNITS_PER_EWATT as f64).round() as u64
}

/// Compute founder time-lock: outputs mined before block 10,000
/// are spendable only after max(50000, current_block + 40000).
pub fn founder_lock_block(block_number: u64) -> u64 {
    if block_number < constants::RAMP_UP_BLOCKS {
        std::cmp::max(constants::FOUNDER_LOCK_BLOCKS, block_number + constants::FOUNDER_LOCK_ADDITIONAL)
    } else {
        0 // no lock after ramp-up
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_founder_lock() {
        assert!(founder_lock_block(500) >= 50000);
        assert_eq!(founder_lock_block(15000), 0);
    }
    #[test] fn test_ewatt_to_units() {
        assert_eq!(ewatt_to_units(100.0), 100_000_000);
        assert_eq!(ewatt_to_units(0.0), 0);
        assert_eq!(ewatt_to_units(0.000001), 1);
        assert_eq!(ewatt_to_units(99.999999), 99_999_999);
        assert_eq!(ewatt_to_units(0.0000005), 1);  // rounding up
    }
    #[test] fn test_reward_proportional_int() {
        // Two miners with same effective commitment → equal rewards (integer math)
        let eff: u64 = 100_000_000_000;  // 100 GB/s * COMMIT_PRECISION
        let em = compute_emission_rate_int(eff * 2, eff * 2);  // total=2×each, avg=2×each
        let commits = vec![(eff, [1u8;32]), (eff, [2u8;32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert_eq!(rewards[0].1, rewards[1].1, "Equal miners get equal rewards");
        assert!(rewards[0].1 > 0, "Rewards must be positive");
    }
    #[test] fn test_reward_honest_more_int() {
        // Honest (100 GB/s effective) vs under-declarer (10 GB/s effective)
        // Honest gets 100/110 ≈ 90.9% of total reward (direct proportion)
        let honest_eff: u64 = 100_000_000_000;
        let under_eff: u64 = 10_000_000_000;
        let total_eff = honest_eff + under_eff;
        let em = compute_emission_rate_int(total_eff, total_eff);
        let commits = vec![(honest_eff, [1u8;32]), (under_eff, [2u8;32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert!(rewards[0].1 > rewards[1].1, "Honest miner earns more");
        let ratio = rewards[0].1 as f64 / (rewards[0].1 + rewards[1].1) as f64;
        assert!((ratio - 0.909).abs() < 0.01, "Honest share ~90.9%, got {}", ratio);
    }
    #[test] fn test_solo_miner_reward_positive() {
        // Solo miner at ramp-up: should get positive reward
        let eff: u64 = 100_000_000_000;
        let em = compute_emission_rate_int(eff, eff);
        let commits = vec![(eff, [1u8;32])];
        let rewards = compute_block_rewards_int(5000, &commits, em);
        assert!(!rewards.is_empty(), "Solo miner should get reward");
        assert!(rewards[0].1 > 0, "Reward must be positive");
    }
}
