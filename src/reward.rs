use crate::constants;

/// Emission rate = BASE * total_eff / hist_avg, clamped, in EMISSION_PRECISION
pub fn compute_emission_rate_int(total_eff: u64, hist_avg: u64) -> u64 {
    if hist_avg == 0 { return crate::constants::BASE_EMISSION_INT; }
    let rate = crate::constants::BASE_EMISSION_INT
        .saturating_mul(total_eff) / hist_avg;
    let floor = crate::constants::BASE_EMISSION_INT
        .saturating_mul(crate::constants::EMISSION_FLOOR_MULTIPLIER_INT) / crate::constants::EMISSION_PRECISION;
    let ceil = crate::constants::BASE_EMISSION_INT
        .saturating_mul(crate::constants::EMISSION_CEILING_MULTIPLIER_INT) / crate::constants::EMISSION_PRECISION;
    rate.clamp(floor, ceil)
}

/// Cap individual miner share during ramp-up, returning excess burned
pub fn apply_ramp_up_cap_int(block_number: u64, rewards: &mut Vec<(Vec<u8>, u64)>) -> u64 {
    if block_number >= crate::constants::RAMP_UP_BLOCKS {
        return 0;
    }
    let total: u64 = rewards.iter().map(|(_, r)| r).sum();
    if total == 0 { return 0; }
    let mut burned = 0u64;
    for (_, reward) in rewards.iter_mut() {
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

/// Compute per-miner rewards proportional to effective commitment, in base units
pub fn compute_block_rewards_int(
    block_number: u64,
    commitments: &[(u64, [u8; 32])],
    emission_rate_int: u64,
) -> Vec<(Vec<u8>, u64)> {
    let total_eff: u64 = commitments.iter().map(|(c, _)| c).sum();
    if total_eff == 0 { return vec![]; }
    let mut rewards: Vec<(Vec<u8>, u64)> = commitments.iter().map(|(c, mid)| {
        let r = if total_eff > 0 {
            let num = (*c as u128).saturating_mul(emission_rate_int as u128);
            (num / total_eff as u128) as u64
        } else {
            0
        };
        (mid.to_vec(), r)
    }).collect();
    let _burned = apply_ramp_up_cap_int(block_number, &mut rewards);
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

/// Founder outputs locked until max(50000, block + 40000)
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
        let eff: u64 = 100_000_000_000;
        let em = compute_emission_rate_int(eff * 2, eff * 2);
        let commits = vec![(eff, [1u8;32]), (eff, [2u8;32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert_eq!(rewards[0].1, rewards[1].1);
        assert!(rewards[0].1 > 0);
    }
    #[test] fn test_reward_honest_more_int() {
        let honest_eff: u64 = 100_000_000_000;
        let under_eff: u64 = 10_000_000_000;
        let total_eff = honest_eff + under_eff;
        let em = compute_emission_rate_int(total_eff, total_eff);
        let commits = vec![(honest_eff, [1u8;32]), (under_eff, [2u8;32])];
        let rewards = compute_block_rewards_int(20000, &commits, em);
        assert!(rewards[0].1 > rewards[1].1);
        let ratio = rewards[0].1 as f64 / (rewards[0].1 + rewards[1].1) as f64;
        assert!((ratio - 0.909).abs() < 0.01);
    }
    #[test] fn test_solo_miner_reward_positive() {
        let eff: u64 = 100_000_000_000;
        let em = compute_emission_rate_int(eff, eff);
        let commits = vec![(eff, [1u8;32])];
        let rewards = compute_block_rewards_int(5000, &commits, em);
        assert!(!rewards.is_empty());
        assert!(rewards[0].1 > 0);
    }
}
